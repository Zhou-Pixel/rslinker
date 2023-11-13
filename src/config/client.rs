use serde::{Deserialize, Serialize};
use toml::{Value, map::Map};

#[derive(Debug, Deserialize, Serialize)]
pub struct MultipleClient {
    pub client: Vec<Client>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SingleClient {
    pub client: Client,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Client {
    pub server_addr: String,
    pub server_port: u16,
    #[serde(default)]
    pub accept_conflict: bool,

    pub heartbeat_interval: Option<u64>,

    pub retry_times: u32,

    pub link: Vec<Link>,
    pub protocol: String,

    #[serde(default)]
    pub tcp_config: TcpConfig,

    pub quic_config: Option<QuicConfig>,
    pub tls_config: Option<TlsConfig>,
    pub kcp_config: Option<KcpConfig>,
    #[serde(flatten)]
    pub extra: Map<String, Value>
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Link {
    pub local_addr: String,
    pub local_port: u16,
    pub remote_port: u16,
    pub protocol: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TcpConfig {
    pub nodelay: bool,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self { nodelay: true }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QuicConfig {
    pub ca: String,
    pub server_name: Option<String>,

    #[serde(default)]
    pub enable_client_auth: bool,
    pub cert: Option<String>,
    pub key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TlsConfig {
    pub ca: String,
    pub server_name: Option<String>,

    #[serde(default)]
    pub enable_client_auth: bool,
    pub cert: Option<String>,
    pub key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KcpConfig {

}
