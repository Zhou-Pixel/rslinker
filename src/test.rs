use rslinker::config::client::*;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};


fn make_client_config() -> Configure {
    Configure {
        client: vec![
            Client {
                server_addr: "127.0.0.1".to_string(),
                server_port: 33445,
                accept_conflict: false,
                link: vec![
                    Link {
                        client_port: 56,
                        server_port: 55,
                        protocol: "udp".to_string(),
                    },
                    Link {
                        client_port: 5556,
                        server_port: 5525,
                        protocol: "udp".to_string(),
                    },
                ],
                protocol: "tls".to_string(),
                tcp_config: Some(TcpConfig {
                }),
                quic_config: None,
                tls_config: None,
            },
            Client {
                server_addr: "127.0.0.1".to_string(),
                server_port: 33445,
                accept_conflict: false,
                link: vec![
                    Link {
                        client_port: 5126,
                        server_port: 535,
                        protocol: "tcp".to_string(),
                    },
                    Link {
                        client_port: 5556,
                        server_port: 5525,
                        protocol: "udp".to_string(),
                    },
                ],
                protocol: "tcp".to_string(),
                tcp_config: Some(TcpConfig {
                }),
                quic_config: None,
                tls_config: None,
            },
        ],
    }
}


#[tokio::test]
async fn write_client_config_json() -> anyhow::Result<()> {
    let mut opt = fs::OpenOptions::new();
    opt.create(true).write(true);
    let mut file = opt.open("./client.json").await?;
    let config = make_client_config();
    let config = serde_json::to_string_pretty(&config)?;
    file.write_all(config.as_bytes()).await?;
    Ok(())
}

#[tokio::test]
async fn write_client_config_toml() -> anyhow::Result<()> {
    let mut opt = fs::OpenOptions::new();
    opt.create(true).write(true);
    let mut file = opt.open("./client.toml").await?;
    let config = make_client_config();
    let config = toml::to_string(&config)?;
    file.write_all(config.as_bytes()).await?;
    Ok(())
}

#[tokio::test]
async fn read_client_config_toml() -> anyhow::Result<()> {
    let mut file = fs::File::open("./client.toml").await?;
    let mut content = String::new();
    file.read_to_string(&mut content).await?;
    let config: Configure = toml::from_str(&content)?;
    println!("config is: {:#?}", config);
    Ok(())
}