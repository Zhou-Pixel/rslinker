use serde::{ Deserialize, Serialize };

#[derive(Debug, Deserialize, Serialize)]
pub struct Configure {
    pub client: Vec<Client>
}

const fn accept_conflict_default_value() -> bool { false }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Client {
    pub server_addr: String,
    pub server_port: u16,
    #[serde(default="accept_conflict_default_value")]
    pub accept_conflict: bool,
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



