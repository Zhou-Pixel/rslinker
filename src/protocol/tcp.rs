use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

use super::{Safe, SimpleRead, SimpleStream, SimpleWrite};

#[derive(Default)]
pub struct TcpFactory;


impl Safe for TcpFactory {}

impl Safe for TcpListener {}

impl Safe for TcpStream {}

#[async_trait::async_trait]
impl super::Factory for TcpFactory {
    type Socket = TcpStream;
    type Listener = TcpListener;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Listener> {
        anyhow::Ok(TcpListener::bind(addr).await?)
    }
    async fn accept(&self, listener: &Self::Listener) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        anyhow::Ok(listener.accept().await?)
    }

    async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        anyhow::Ok(TcpStream::connect(addr).await?)
    }
}

impl SimpleStream for TcpStream {
    
}

impl SimpleRead for TcpStream {

}

impl SimpleWrite for TcpStream {
    
}