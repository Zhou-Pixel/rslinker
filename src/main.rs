use clap::{arg, Parser};
use rslinker::{
    config,
    protocol::{
        quic::{ClientCertification, QuicFactory, ServerCertification},
        tcp::TcpFactory,
        tls::{TlsClientConfig, TlsFactory, TlsServerConfig},
        Address, BasicProtocol, Factory, Port, Verification
    },
    utils,
};
use simplelog::{CombinedLogger, Config, TermLogger};
use std::{collections::HashMap, path::PathBuf};
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
        run_client(config).await;
    }

    handle_ctrl_c().await?;
    Ok(())
}

async fn read_config(path: PathBuf) -> io::Result<String> {
    let mut file: File = tokio::fs::File::open(path).await?;

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

async fn run_server(config: config::server::Server) -> anyhow::Result<()> {
    use rslinker::server::Server;
    let addr = format!("{}:{}", config.addr, config.port);
    let addr = Address::resolve(&addr).await?;
    log::info!("server binding addr is: {}", addr);

    match config.protocol.as_str() {
        "tcp" => {
            let mut tcpfactory = TcpFactory::default();
            tcpfactory.set_nodelay(config.tcp_config.nodelay);
            let server: Server<TcpFactory> = Server::new(addr, tcpfactory);
            server.run();
            log::info!("tcp server is running");
        }
        "quic" => {
            let mut quic = QuicFactory::new();
            let quic_config = config
                .quic_config
                .as_ref()
                .ok_or(anyhow::anyhow!("Quic config must be set"))?;
            let crt = tokio::fs::read(quic_config.cert.as_str()).await?;
            let key = tokio::fs::read(quic_config.key.as_str()).await?;
            quic.server = Some(ServerCertification { crt, key });

            let server: Server<QuicFactory> = Server::new(addr, quic);
            server.run();
            log::info!("quic server is running");
        }
        "tls" => {
            let mut tls_factory = TlsFactory::new();
            let tls_config = config
                .tls_config
                .as_ref()
                .ok_or(anyhow::anyhow!("tls config must be set"))?;
            let certs = utils::load_certs(tls_config.cert.as_str()).await?;
            let key = utils::load_private_key(tls_config.key.as_str()).await?;

            let ca = match tls_config.ca {
                Some(ref ca) => {
                    let bytes = tokio::fs::read(ca.as_str()).await?;
                    Some(rustls::Certificate(bytes))
                }
                None => None,
            };
            tls_factory.server_config = Some(TlsServerConfig {
                verification: Verification { certs, key },
                ca,
                enable_client_auth: tls_config.enable_client_auth,
            });
            let server = Server::new(addr, tls_factory);
            server.run();
        }
        _ => unimplemented!(),
    }
    Ok(())
}

async fn run_client(config: config::client::Configure) {
    for i in config.client {
        tokio::spawn(async move {
            let result = async move {
                let protocol = i.protocol.clone();

                let addr = format!("{}:{}", i.server_addr, i.server_port);
                log::info!("Start to resolve address");
                let addr = Address::resolve(&addr).await?;
                log::info!("client addr is {}", addr);
                match i.protocol.as_str() {
                    "tcp" => {
                        let mut tcpfactory = TcpFactory::default();
                        tcpfactory.set_nodelay(i.tcp_config.nodelay);
                        run_client_with_config(addr.clone(), tcpfactory, i).await?;
                    }
                    "tls" => {
                        let mut tls_factory = TlsFactory::default();

                        let tls_config = i
                            .tls_config
                            .as_ref()
                            .ok_or(anyhow::anyhow!("Tls config must be set"))?;

                        let server_name = tls_config.server_name.as_ref().unwrap_or(&i.server_addr);

                        let veri = match (
                            tls_config.enable_client_auth,
                            &tls_config.cert,
                            &tls_config.key,
                        ) {
                            (true, Some(cert), Some(key)) => {
                                let certs = utils::load_certs(cert.as_str()).await?;
                                let key = utils::load_private_key(key.as_str()).await?;
                                Some(Verification { certs, key })
                            }
                            (false, _, _) => None,
                            _ => anyhow::bail!(
                                "cert and key must be set if enable_client_auth is true"
                            ),
                        };

                        let config = TlsClientConfig {
                            server_name: server_name.to_owned(),
                            verification: veri,
                            enable_client_auth: tls_config.enable_client_auth,
                            ca: Some(utils::load_certs(tls_config.ca.as_str()).await?.remove(0)),
                        };

                        tls_factory.client_config = Some(config);

                        run_client_with_config(addr.clone(), tls_factory, i).await?;
                    }
                    "quic" => {
                        let mut quicfactory = QuicFactory::new();
                        let quic_config = i
                            .quic_config
                            .as_ref()
                            .ok_or(anyhow::anyhow!("quic config must be set"))?;
                        let crt = tokio::fs::read(quic_config.cert.as_str()).await?;
                        quicfactory.client = Some(ClientCertification {
                            crt,
                            server_name: quic_config.server_name.clone(),
                        });
                        run_client_with_config(addr.clone(), quicfactory, i).await?;
                    }
                    _ => {
                        log::warn!("not support protocol {}", protocol);
                        anyhow::bail!("not support protocol {}", protocol);
                    }
                }
                anyhow::Ok(())
            }
            .await;
            log::info!("Client exit, state: {result:?}");
        });
    }
}

async fn run_client_with_config<T: Factory>(
    addr: Address,
    factory: T,
    config: rslinker::config::client::Client,
) -> anyhow::Result<()> {
    use rslinker::client::Client;
    let mut client = Client::connect(addr, factory).await?;

    let mut links = HashMap::new();

    for j in &config.link {
        let protocol: BasicProtocol = match j.protocol.as_str() {
            "tcp" => BasicProtocol::Tcp,
            "ssh" => BasicProtocol::Tcp,
            "tls" => BasicProtocol::Tcp,
            "udp" => BasicProtocol::Udp,
            "quic" => BasicProtocol::Udp,
            _ => {
                log::warn!("Unknown protocol {}", j.protocol);
                continue;
            }
        };
        let addr = format!("{}:{}", j.local_addr, j.local_port);
        let Ok(addr) = Address::resolve(&addr).await else {
            log::info!("Invalid addr: {}", addr);
            continue;
        };
        links.insert(
            Port {
                port: j.remote_port,
                protocol,
            },
            addr.socketaddr(),
        );
    }

    client.set_accept_conflict(config.accept_conflict);
    client.set_links(links);
    client.set_retry_times(config.retry_times);
    client.exec().await
}

async fn handle_ctrl_c() -> anyhow::Result<()> {
    use tokio::signal;
    signal::ctrl_c().await?;
    Ok(())
}
