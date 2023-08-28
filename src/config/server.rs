use serde::{Deserialize, Serialize};


#[derive(Deserialize, Serialize)]
pub struct Server {
    pub port: u16,
    pub addr: Option<String>,
    pub protocol: String,
    pub tcp_config: Option<Tcp>,
    pub quic_config: Option<Quic>,
    pub tls_config: Option<Tls>
}


#[derive(Deserialize, Serialize)]
pub struct Tcp {
    pub token: Option<String>,
}


#[derive(Deserialize, Serialize)]
pub struct Quic {

}


#[derive(Deserialize, Serialize)]
pub struct Tls {

}

