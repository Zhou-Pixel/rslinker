use clap::{arg, Parser, Subcommand};
use rslinker::{
    client::{Client, TransferProtocol},
    config::{self, server::SingleServer},
    protocol::{
        kcp::KcpFactory,
        quic::{QuicClientConfig, QuicFactory, QuicServerConfig},
        tcp::TcpFactory,
        tls::{TlsClientConfig, TlsFactory, TlsServerConfig},
        BasicProtocol, CacheAddr, Factory, Port, Verification,
    },
    server::Server,
    utils,
};
use unicase::Ascii;
use simplelog::{CombinedLogger, Config, LevelFilter, TermLogger};
use std::{collections::HashMap, path::PathBuf};
use tokio_kcp::KcpConfig;
use toml::{map::Map, Value};

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

        #[arg(short, long, value_name = "MULTIPLE MODE")]
        multiple: bool,

        #[arg(short, long, value_name = "LOG_LEVEL")]
        level: Option<String>,
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
            level,
            ..
        } => {
            let level = level.unwrap_or("info".to_string());

            let level = match level.as_str() {
                "off" => LevelFilter::Off,
                "error" => LevelFilter::Error,
                "warn" => LevelFilter::Warn,
                "info" => LevelFilter::Info,
                "debug" => LevelFilter::Debug,
                "trace" => LevelFilter::Trace,
                _ => {
                    eprintln!("[WARN]: Unknow Level is specified: {level}, default to info");
                    LevelFilter::Info
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

            if server {
                let config = config::make_server_config(path).await?;
                log::info!("Prepare to run as server");
                run_server(config).await?;
            } else {
                let config = config::make_client_config(path).await?;
                log::info!("Prepare to run as client");
                run_client(config.client).await;
            }

            handle_ctrl_c().await?;
        }
    }

    Ok(())
}

fn log_init(level: LevelFilter) -> anyhow::Result<()> {
    CombinedLogger::init(vec![TermLogger::new(
        level,
        Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )])?;
    Ok(())
}

async fn run_server(config: SingleServer) -> anyhow::Result<()> {
    let server_config = config.server;
    let addr = format!("{}:{}", server_config.addr, server_config.port).parse()?;
    log::info!("Server binding addr is: {}", addr);

    match server_config.protocol.as_str() {
        "tcp" => {
            let mut tcpfactory = TcpFactory::default();
            tcpfactory.set_nodelay(server_config.tcp_config.nodelay);
            let server: Server<TcpFactory> = Server::new(addr, tcpfactory);
            server.run();
            log::info!("Tcp server is running");
        }
        "quic" => {
            let mut quic_factory = QuicFactory::new();
            let quic_config = server_config
                .quic_config
                .as_ref()
                .ok_or(anyhow::anyhow!("Quic config must be set"))?;
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
            log::info!("Quic server is running");
        }
        "tls" => {
            let mut tls_factory = TlsFactory::new();
            let tls_config = server_config
                .tls_config
                .as_ref()
                .ok_or(anyhow::anyhow!("Tls config must be set"))?;
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
            log::info!("Tls server is running");
        }
        "kcp" => {
            let config = KcpConfig::default();

            let kcp_factory = KcpFactory::from_config(config);
            let server = Server::new(addr, kcp_factory);
            server.run();
            log::info!("Kcp server is running");
        }
        _ => {
            log::error!("The protocol is currently not supported");
        }
    }
    Ok(())
}

async fn run_client(configs: Vec<config::client::Client>) {
    for i in configs {
        tokio::spawn(async move {
            let result = async move {
                let protocol = i.protocol.clone();

                let addr = format!("{}:{}", i.server_addr, i.server_port);
                log::info!("Start to resolve address");
                let addr = CacheAddr::resolve(&addr).await?;
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
                                "Cert and key must be set if enable_client_auth is true"
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
                    "kcp" => {
                        let config = KcpConfig::default();

                        let kcp_factory = KcpFactory::from_config(config);

                        run_client_with_config(addr, kcp_factory, i).await?;
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
    addr: CacheAddr,
    factory: T,
    config: config::client::Client,
) -> anyhow::Result<()> {
    let mut links = HashMap::new();

    for j in &config.link {
        let addr = format!("{}:{}", j.local_addr, j.local_port);
        let Ok(addr) = CacheAddr::resolve(&addr).await else {
            log::info!("Invalid addr: {}", addr);
            continue;
        };
        let (transfer, basic) = match j.protocol.as_str() {
            "tcp" => (TransferProtocol::RawTcp(addr), BasicProtocol::Tcp),
            "ssh" => (TransferProtocol::RawTcp(addr), BasicProtocol::Tcp),
            "tls" => (TransferProtocol::RawTcp(addr), BasicProtocol::Tcp),
            "http" => {
                let auto_override = j
                    .extra
                    .get("auto_override")
                    .unwrap_or(&Value::Boolean(true))
                    .as_bool()
                    .ok_or(anyhow::anyhow!("auto_override must be bool value"))?;

                let r#override = Value::Table(Map::new());
                let r#override = j
                    .extra
                    .get("override")
                    .unwrap_or(&r#override)
                    .as_table()
                    .ok_or(anyhow::anyhow!(
                        "override must be a table that contains key(string) and value(string)"
                    ))?;

                let mut override_ = HashMap::new();

                for (key, value) in r#override {
                    let value = value.as_str().ok_or(anyhow::anyhow!("http override value must be str"))?;
                    override_.insert(Ascii::new(key.to_string()), value.to_string());
                }

                (TransferProtocol::Http { auto_override, override_, addr }, BasicProtocol::Tcp)
            }
            "udp" => (TransferProtocol::RawUdp(addr), BasicProtocol::Udp),
            "quic" => (TransferProtocol::RawUdp(addr), BasicProtocol::Udp),
            "kcp" => (TransferProtocol::RawUdp(addr), BasicProtocol::Udp),
            _ => {
                log::error!("Unknown protocol:{}, Skip", j.protocol);
                continue;
            }
        };
        links.insert(Port {
            port: j.remote_port,
            protocol: basic,
        }, transfer);
    }

    let client_config = rslinker::client::ClientConfig {
        links,
        accept_conflict: config.accept_conflict,
        retry_times: config.retry_times,
        heartbeat_interval: config.heartbeat_interval,
    };
    let mut client = Client::connect(addr, factory, client_config).await?;
    client.exec().await
}

async fn handle_ctrl_c() -> anyhow::Result<()> {
    use tokio::signal;
    signal::ctrl_c().await?;
    Ok(())
}
