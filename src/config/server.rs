use serde::{Deserialize, Serialize};

fn addr_default_value() -> String {
    "0.0.0.0".to_string()
}

#[derive(Deserialize, Serialize)]
pub struct SingleServer {
    pub server: Server,

}

pub struct MultipleServer {
    pub server: Vec<Server>
}

#[derive(Deserialize, Serialize)]
pub struct Server {
    pub port: u16,
    #[serde(default = "addr_default_value")]
    pub addr: String,
    pub protocol: String,

    #[serde(default)]
    pub tcp_config: TcpConfig,
    pub quic_config: Option<QuicConfig>,
    pub tls_config: Option<TlsConfig>,
    pub kcp_config: Option<KcpConfig>
}

#[derive(Deserialize, Serialize)]
pub struct TcpConfig {
    pub nodelay: bool,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self { nodelay: true }
    }
}

#[derive(Deserialize, Serialize)]
pub struct QuicConfig {
    pub ca: Option<String>,

    #[serde(default)]
    pub enable_client_auth: bool,
    pub cert: String,
    pub key: String,
}

#[derive(Deserialize, Serialize)]
pub struct TlsConfig {
    pub ca: Option<String>,

    #[serde(default)]
    pub enable_client_auth: bool,
    pub cert: String,
    pub key: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct KcpConfig {

}