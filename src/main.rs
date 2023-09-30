use clap::{arg, Parser, Subcommand};
use rslinker::{
    config,
    protocol::{
        quic::{QuicClientConfig, QuicFactory, QuicServerConfig},
        tcp::TcpFactory,
        tls::{TlsClientConfig, TlsFactory, TlsServerConfig},
        Address, BasicProtocol, Factory, Port, Verification,
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
    #[command(subcommand)]
    command: SubCommand,
}

#[derive(Subcommand, Debug)]
enum SubCommand {
    Run {
        #[arg(long)]
        server: bool,

        #[arg(long)]
        client: bool,

        #[arg(short, long, value_name = "CONFIG")]
        config: Option<PathBuf>,

        #[arg(short, long, value_name = "LOG_LEVEL")]
        level: Option<String>
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cmd = Command::parse();


    match cmd.command {
        SubCommand::Run {
            server,
            client,
            config,
            level
        } => {


            let level = level.unwrap_or("info".to_string());

            let level = match level.as_str() {
                "off" => simplelog::LevelFilter::Off,
                "error" => simplelog::LevelFilter::Error,
                "warn" => simplelog::LevelFilter::Warn,
                "info" => simplelog::LevelFilter::Info,
                "debug" => simplelog::LevelFilter::Debug,
                "trace" => simplelog::LevelFilter::Trace,
                _ => {
                    eprintln!("warn: Unknow Leve is specified: {level}, default to info");
                    simplelog::LevelFilter::Info
                }
            };

            log_init(level)?;

            if server && client {
                log::error!("This program can only run as server or client");
                return Ok(());
            }

            let path = if server {
                config.unwrap_or(PathBuf::from("./server.toml"))
            } else {
                config.unwrap_or(PathBuf::from("./client.toml"))
            };

            log::info!("Config path is {:?}", path);

            let content = read_config(path).await?;

            if server {
                let config = toml::from_str::<config::server::Configuration>(&content)?;
                log::info!("Prepare to run in server mode");
                run_server(config).await?;
            } else {
                let config: config::client::Configuration = toml::from_str(&content)?;
                log::info!("Prepare to run in client mode");
                run_client(config).await;
            }

            handle_ctrl_c().await?;
        }
    }

    Ok(())
}

async fn read_config(path: PathBuf) -> io::Result<String> {
    let mut file: File = tokio::fs::File::open(path).await?;

    let mut content = String::new();

    file.read_to_string(&mut content).await?;
    Ok(content)
}

fn log_init(level: simplelog::LevelFilter) -> anyhow::Result<()> {
    CombinedLogger::init(vec![TermLogger::new(
        level,
        Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )])?;
    Ok(())
}

async fn run_server(config: config::server::Configuration) -> anyhow::Result<()> {
    use rslinker::server::Server;
    let server_config = config.server;
    let addr = format!("{}:{}", server_config.addr, server_config.port);
    let addr = Address::resolve(&addr).await?;
    log::info!("server binding addr is: {}", addr);

    match server_config.protocol.as_str() {
        "tcp" => {
            let mut tcpfactory = TcpFactory::default();
            tcpfactory.set_nodelay(config.tcp_config.nodelay);
            let server: Server<TcpFactory> = Server::new(addr, tcpfactory);
            server.run();
            log::info!("tcp server is running");
        }
        "quic" => {
            let mut quic_factory = QuicFactory::new();
            let quic_config = config
                .quic_config
                .as_ref()
                .ok_or(anyhow::anyhow!("quic config must be set"))?;
            let certs = utils::load_certs(quic_config.cert.as_str()).await?;
            let key = utils::load_private_key(quic_config.key.as_str()).await?;

            let ca = match quic_config.ca {
                Some(ref ca) => Some(utils::load_certs(ca.as_str()).await?.remove(0)),
                None => None,
            };
            quic_factory.server_config = Some(QuicServerConfig {
                verification: Verification { certs, key },
                ca,
                enable_client_auth: quic_config.enable_client_auth,
            });

            let server: Server<QuicFactory> = Server::new(addr, quic_factory);
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
                    // let bytes = tokio::fs::read(ca.as_str()).await?;
                    Some(utils::load_certs(ca.as_str()).await?.remove(0))
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

async fn run_client(config: config::client::Configuration) {
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
                        let mut quic_factory = QuicFactory::new();

                        let quic_config = i
                            .quic_config
                            .as_ref()
                            .ok_or(anyhow::anyhow!("Quic config must be set"))?;

                        let server_name =
                            quic_config.server_name.as_ref().unwrap_or(&i.server_addr);

                        let veri = match (
                            quic_config.enable_client_auth,
                            &quic_config.cert,
                            &quic_config.key,
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

                        let config = QuicClientConfig {
                            server_name: server_name.to_owned(),
                            verification: veri,
                            enable_client_auth: quic_config.enable_client_auth,
                            ca: Some(utils::load_certs(quic_config.ca.as_str()).await?.remove(0)),
                        };

                        quic_factory.client_config = Some(config);

                        run_client_with_config(addr.clone(), quic_factory, i).await?;
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
