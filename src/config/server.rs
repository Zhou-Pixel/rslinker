use serde::{Deserialize, Serialize};


#[derive(Deserialize, Serialize)]
pub struct Server {
    pub port: u16,
    pub addr: Option<String>,
    pub protocol: String,
    pub tcp_config: Option<TcpConfig>,
    pub quic_config: Option<QuicConfig>,
    pub tls_config: Option<TlsConfig>
}


#[derive(Deserialize, Serialize)]
pub struct TcpConfig {
}


#[derive(Deserialize, Serialize)]
pub struct QuicConfig {
    pub crt: String,
    pub key: String,
}


#[derive(Deserialize, Serialize)]
pub struct TlsConfig {

}

