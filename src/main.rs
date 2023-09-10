use clap::{arg, Parser};
use rslinker::{
    config,
    protocol::{quic::QuicFactory, tcp::TcpFactory, BasicProtocol, Port},
};
use simplelog::{CombinedLogger, Config, TermLogger};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt},
};

#[cfg(test)]
mod test;

#[derive(Debug, Parser)]
struct Command {
    #[arg(long)]
    server: bool,

    #[arg(long)]
    client: bool,

    #[arg(short, long, value_name = "CONFIG")]
    config: Option<PathBuf>,
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cmd = Command::parse();

    log_init()?;

    if cmd.server && cmd.client {
        log::error!("Only one mode can be selected");
        return Ok(());
    }

    let path = if cmd.server {
        cmd.config.unwrap_or(PathBuf::from("./server.toml"))
    } else {
        cmd.config.unwrap_or(PathBuf::from("./client.toml"))
    };

    let content = read_config(path).await?;

    if cmd.server {
        let config = toml::from_str::<config::server::Server>(&content)?;
        run_server(config).await?;
    } else {
        let config: config::client::Configure = toml::from_str(&content)?;
        run_client(config);
    }

    handle_ctrl_c().await?;
    Ok(())
}

async fn read_config(path: PathBuf) -> io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;

    let mut content = String::new();

    file.read_to_string(&mut content).await?;
    Ok(content)
}

fn log_init() -> anyhow::Result<()> {
    CombinedLogger::init(vec![TermLogger::new(
        simplelog::LevelFilter::Info,
        Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )])?;
    Ok(())
}

async fn read_all(path: &str) -> anyhow::Result<String> {
    let mut file = File::open(path).await?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).await?;
    Ok(buf)
}

async fn run_server(config: config::server::Server) -> anyhow::Result<()> {
    use rslinker::server::Server;
    // use rslinker::protocol::tcp::TcpOptions;
    let addr = config.addr.unwrap_or("0.0.0.0".to_string());
    let addr: SocketAddr = format!("{}:{}", addr, config.port).parse().unwrap();
    log::info!("server binding addr is: {}", addr);
    // if let Some(ref tcp_config) = config.tcp_config {
    //     if let Some(ref token) = tcp_config.token {
    //         let cipher = Aes256::new(token.as_bytes());
    //         TcpOptions::instance().write().await.set_cipher(Some(cipher));
    //     }
    // }
    if config.protocol == "tcp" {
        let server: Server<TcpFactory> = Server::new(addr, Default::default());
        server.run();
        log::info!("tcp server is running");
    } else if config.protocol == "quic" {
        let mut quic = QuicFactory::new();
        let quic_config = config.quic_config.as_ref().unwrap();
        let crt = read_all(quic_config.crt.as_str()).await?;
        let key = read_all(quic_config.key.as_str()).await?;
        quic.set_crt(crt);
        quic.set_key(key);

        let server: Server<QuicFactory> = Server::new(addr, quic);
        server.run();
        log::info!("quic server is running");
    } else {
        unimplemented!();
    }
    Ok(())
}

fn run_client(config: config::client::Configure) {
    for i in config.client {
        let addr: SocketAddr = format!("{}:{}", i.server_addr, i.server_port)
            .parse()
            .unwrap();
        log::info!("client addr is {}", addr);
        let mut links = HashMap::new();
        for j in &i.link {
            let protocol = match j.protocol.as_str() {
                "tcp" => BasicProtocol::Tcp,
                "udp" => BasicProtocol::Udp,
                _ => {
                    log::warn!("unknown protocol {}", j.protocol);
                    continue;
                }
            };
            links.insert(
                Port {
                    port: j.server_port,
                    protocol,
                },
                j.client_port,
            );
        }
        let protocol = i.protocol.clone();
        let accept_conflict = i.accept_conflict;
        tokio::spawn(async move {
            use rslinker::client::Client;
            let result = match protocol.as_str() {
                "tcp" => {
                    log::info!("starting a new tcp tcp client, config: {:?}", links);
                    let mut client: Client<TcpFactory> =
                        Client::connect(addr, Default::default()).await?;
                    client.set_accept_conflict(accept_conflict);
                    client.set_links(links);
                    client.exec().await
                }
                "tls" => {
                    unimplemented!("tls protocol");
                }
                "quic" => {
                    let mut quic = QuicFactory::new();
                    let quic_config = i.quic_config.as_ref().unwrap();
                    let crt = read_all(quic_config.crt.as_str()).await?;
                    quic.set_crt(crt);
                    quic.set_server_name(quic_config.server_name.clone());

                    let mut client: Client<QuicFactory> = Client::connect(addr, quic).await?;
                    client.set_accept_conflict(accept_conflict);
                    client.set_links(links);
                    client.exec().await
                }
                _ => {
                    log::warn!("not support protocol {}", protocol);
                    return Err(anyhow::anyhow!("not support protocol"));
                }
            };
            if result.is_err() {
                log::error!("Disconnected server_addr: {}, error:{:?}", addr, result.unwrap_err());
            }
            anyhow::Ok(())
        });
    }
}

async fn handle_ctrl_c() -> anyhow::Result<()> {
    use tokio::signal;
    signal::ctrl_c().await?;
    Ok(())

}
