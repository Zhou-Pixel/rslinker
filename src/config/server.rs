use serde::{Deserialize, Serialize};


fn addr_default_value() -> String {
    "0.0.0.0".to_string()
}

#[derive(Deserialize, Serialize)]
pub struct Server {
    pub port: u16,
    #[serde(default="addr_default_value")]
    pub addr: String,
    pub protocol: String,
    
    #[serde(default="tcp_config_default_value")]
    pub tcp_config: TcpConfig,
    pub quic_config: Option<QuicConfig>,
    pub tls_config: Option<TlsConfig>
}


const fn nodelay_default_value() -> bool { true }

const fn tcp_config_default_value() -> TcpConfig {
    TcpConfig {
        nodelay: true
    }
}

#[derive(Deserialize, Serialize)]
pub struct TcpConfig {
    #[serde(default="nodelay_default_value")]
    pub nodelay: bool
}


#[derive(Deserialize, Serialize)]
pub struct QuicConfig {
    pub cert: String,
    pub key: String,
}


#[derive(Deserialize, Serialize)]
pub struct TlsConfig {

    pub ca: Option<String>,

    #[serde(default)]
    pub enable_client_auth: bool,
    pub cert: String,
    pub key: String
}

