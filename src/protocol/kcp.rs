use super::{Factory, AntiStickyStream};
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

#[async_trait::async_trait]
impl Factory for KcpFactory {
    type Socket = AntiStickyStream<KcpStream>;
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


        Ok((AntiStickyStream::new(socket), addr))
    }
    async fn make(&self) -> anyhow::Result<Self::Connector> {
        Ok(())
    }

    async fn connect(&self, _: &Self::Connector, addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        let socket = KcpStream::connect(&self.config, addr).await?;
        Ok(AntiStickyStream::new(socket))
    }
}
