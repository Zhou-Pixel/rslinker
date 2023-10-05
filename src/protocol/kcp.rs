use super::Factory;
use std::net::SocketAddr;
use tokio_kcp::{KcpConfig, KcpListener, KcpStream};

use fast_async_mutex::RwLock;

pub struct KcpFactory {
    config: KcpConfig,
}

impl KcpFactory {
    pub fn from_config(config: KcpConfig) -> Self {
        Self { config }
    }
}

pub struct KcpSocket {
    socket: KcpStream,
    size: Option<usize>,
    buf: bytes::BytesMut,
}

impl KcpSocket {
    fn new(socket: KcpStream) -> Self {
        Self {
            socket,
            size: None,
            buf: bytes::BytesMut::new(),
        }
    }
}

#[async_trait::async_trait]
impl Factory for KcpFactory {
    type Socket = KcpSocket;
    type Acceptor = RwLock<KcpListener>;
    type Connector = ();
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor> {
        let listener = KcpListener::bind(self.config, addr).await?;
        Ok(RwLock::new(listener))
    }
    async fn accept(
        &self,
        acceptor: &Self::Acceptor,
    ) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        let (socket, addr) = acceptor.write().await.accept().await?;

        let socket = KcpSocket::new(socket);

        Ok((socket, addr))
    }
    async fn make(&self) -> anyhow::Result<Self::Connector> {
        Ok(())
    }

    async fn connect(&self, _: &Self::Connector, addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        let socket = KcpStream::connect(&self.config, addr).await?;
        let socket = KcpSocket::new(socket);
        Ok(socket)
    }
}

impl_async_stream!(KcpSocket, socket);
impl_anti_sticky!(KcpSocket, KcpStream);
