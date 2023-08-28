use crypto2::blockcipher::Aes256;
use simplelog::{CombinedLogger, TermLogger, Config, SharedLogger};
use clap::{arg, Parser};
use tokio::io::{AsyncReadExt, self};
use std::{path::PathBuf, net::SocketAddr, collections::HashMap};
use rslinker::{config, protocol::{tcp::TcpProtocol, Port, BasicProtocol}};


#[derive(Debug, Parser)]
struct Command {
    #[arg(long)]
    server: bool,

    #[arg(long)]
    client: bool,

    #[arg(short, long, value_name="CONFIG")]
    config: Option<PathBuf>
}


#[cfg(test)]
mod test {
    use tokio::{fs, io::{AsyncWriteExt, AsyncReadExt}};
    use rslinker::config::client::*;

    #[tokio::test]
    async fn write_config() -> anyhow::Result<()> {
        let mut opt = fs::OpenOptions::new();
        opt.create(true).write(true);
        let mut file = opt.open("./client.toml").await?;
        let config = Configure {
            client: vec![
                Client {
                    server_addr: "127.0.0.1".to_string(),
                    server_port: 33445,
                    accept_conflict: false,
                    link: vec![
                        Link {
                            client_port: 56,
                            server_port: 55,
                            protocol: "udp".to_string()
                        },
                        Link {
                            client_port: 5556,
                            server_port: 5525,
                            protocol: "udp".to_string()
                        }
                    ],
                    protocol: "tls".to_string(),
                    tcp_config: Some(TcpConfig {
                        token: Some("tcp_token".to_string())
                    }),
                    quic_config: None,
                    tls_config: None
                },
                Client {
                    server_addr: "127.0.0.1".to_string(),
                    server_port: 33445,
                    accept_conflict: false,
                    link: vec![
                        Link {
                            client_port: 5126,
                            server_port: 535,
                            protocol: "tcp".to_string()
                        },
                        Link {
                            client_port: 5556,
                            server_port: 5525,
                            protocol: "udp".to_string()
                        }
                    ],
                    protocol: "tcp".to_string(),
                    tcp_config: Some(TcpConfig {
                        token: Some("tcp_token".to_string())
                    }),
                    quic_config: None,
                    tls_config: None
                }
            ]
        };

        let config = toml::to_string(&config)?;
        file.write_all(config.as_bytes()).await?;
        Ok(())
    }

    #[tokio::test]
    async fn read_config() -> anyhow::Result<()> {
        let mut file = fs::File::open("./client.toml").await?;
        let mut content = String::new();
        file.read_to_string(&mut content).await?;
        let config: Configure = toml::from_str(&content)?;
        println!("config is: {:#?}", config);
        Ok(())
    }
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
        run_server(config).await;
    } else {
        let config: config::client::Configure = toml::from_str(&content)?;
        run_client(config);
    }

    handle_ctrl_c(||{
        log::info!("todo: close application");
        true
    }).await?;
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

async fn run_server(config: config::server::Server) {
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
        let server: Server<TcpProtocol> = Server::new(addr);
        server.run();
        log::info!("tcp server is running");
    } else {
        unimplemented!();
    }
}

fn run_client(config: config::client::Configure) {
    for i in &config.client {
        let addr: SocketAddr = format!("{}:{}", i.server_addr, i.server_port).parse().unwrap();
        log::info!("client addr is {}", addr);
        let mut links = HashMap::new();
        for j in &i.link {
            let protocol = match j.protocol.as_str() {
                "tcp" => BasicProtocol::Tcp,
                "udp" => BasicProtocol::Udp,
                _ => {
                    log::warn!("unknown protocol {}", j.protocol);
                    continue;
                },
            };
            links.insert(Port { port: j.server_port, protocol }, j.client_port);
        }
        let protocol = i.protocol.clone();
        let accept_conflict = i.accept_conflict;
        tokio::spawn(async move {
            use rslinker::client::Client;
            match protocol.as_str() {
                "tcp" => {
                    log::info!("starting a new tcp tcp client, config: {:?}", links);
                    let mut client: Client<TcpProtocol> = Client::connect(addr).await?;
                    client.set_accept_conflict(accept_conflict);
                    client.set_links(links);
                    client.exec().await?;
                },
                "tls" => {
                    unimplemented!("tls protocol");
                },
                "quic" => {
                    unimplemented!("quic protocol");
                },
                _ => {
                    log::warn!("not support protocol {}", protocol);
                    return Err(anyhow::anyhow!("not support protocol"));
                }
            }
            anyhow::Ok(())
        });
    }
}

async fn handle_ctrl_c(f: impl Fn() -> bool + Send + Sync) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use tokio::signal::windows;
        let mut signal = windows::ctrl_c()?;
        loop {
            signal.recv().await;
            if f() {
                break;
            }
        }
        anyhow::Ok(())
    }
    #[cfg(unix)]
    {
        todo!("unix ctrl c handler");
    }
}
