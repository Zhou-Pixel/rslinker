use serde::{ Deserialize, Serialize };

#[derive(Debug, Deserialize, Serialize)]
pub struct Configure {
    pub client: Vec<Client>
}

const fn accept_conflict_default_value() -> bool { false }

const fn heartbeat_interval_default_value() -> u64 { 1000 }

const fn retry_times_default_value() -> u32 { 0 }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Client {
    pub server_addr: String,
    pub server_port: u16,
    #[serde(default="accept_conflict_default_value")]
    pub accept_conflict: bool,

    #[serde(default="heartbeat_interval_default_value")]
    pub heartbeat_interval: u64,

    #[serde(default="retry_times_default_value")]
    pub retry_times: u32, 

    pub link: Vec<Link>,
    pub protocol: String,
    pub tcp_config: Option<TcpConfig>,
    pub quic_config: Option<QuicConfig>,
    pub tls_config: Option<TlsConfig>
}


#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash, Clone)]
pub struct Link {
    pub client_port: u16,
    pub server_port: u16,
    pub protocol: String
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TcpConfig {
}


#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QuicConfig {
    pub crt: String,
    pub server_name: String
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TlsConfig {

}



