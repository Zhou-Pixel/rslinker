use serde::{ Deserialize, Serialize };

#[derive(Debug, Deserialize, Serialize)]
pub struct Configure {
    pub client: Vec<Client>
}

const fn accept_conflict_default_value() -> bool { false }

#[derive(Debug, Deserialize, Serialize)]
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


#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct Link {
    pub client_port: u16,
    pub server_port: u16,
    pub protocol: String
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TcpConfig {
    pub token: Option<String>
}


#[derive(Debug, Deserialize, Serialize)]
pub struct QuicConfig {

}

#[derive(Debug, Deserialize, Serialize)]
pub struct TlsConfig {

}



