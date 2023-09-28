use serde::{ Deserialize, Serialize };

#[derive(Debug, Deserialize, Serialize)]
pub struct Configure {
    pub client: Vec<Client>
}

const fn accept_conflict_default_value() -> bool { false }

const fn heartbeat_interval_default_value() -> u64 { 1000 }

const fn retry_times_default_value() -> u32 { 0 }

const fn tcp_config_default_value() -> TcpConfig {
    TcpConfig {
        nodelay: true
    }
}

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
    
    #[serde(default="tcp_config_default_value")]
    pub tcp_config: TcpConfig,
    
    pub quic_config: Option<QuicConfig>,
    pub tls_config: Option<TlsConfig>
}


#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash, Clone)]
pub struct Link {
    pub local_addr: String,
    pub local_port: u16,
    pub remote_port: u16,
    pub protocol: String
}


const fn nodelay_default_value() -> bool { true }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TcpConfig {
    #[serde(default="nodelay_default_value")]
    pub nodelay: bool
}


#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QuicConfig {
    pub cert: String,
    pub server_name: String
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TlsConfig {
    pub ca: String,
    pub server_name: Option<String>,

    #[serde(default)]
    pub enable_client_auth: bool,
    pub cert: Option<String>,
    pub key: Option<String>
    
}



