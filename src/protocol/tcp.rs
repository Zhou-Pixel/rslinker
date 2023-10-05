use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use bytes::BytesMut;


#[derive(Default)]
pub struct TcpFactory {
    nodelay: bool,
}

impl TcpFactory {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn set_nodelay(&mut self, nodelay: bool) {
        self.nodelay = nodelay;
    }
}

#[async_trait::async_trait]
impl super::Factory for TcpFactory {
    type Socket = TcpSocket;
    type Acceptor = TcpListener;
    type Connector = ();
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor> {
        Ok(TcpListener::bind(addr).await?)
    }

    async fn accept(
        &self,
        listener: &Self::Acceptor,
    ) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        let (socket, addr) = listener.accept().await?;
        socket.set_nodelay(self.nodelay)?;
        Ok((
            TcpSocket {
                socket,
                size: None,
                buf: BytesMut::new(),
            },
            addr,
        ))
    }

    async fn make(&self) -> anyhow::Result<Self::Connector> {
        Ok(())
    }

    async fn connect(&self, _: &Self::Connector, addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        let socket = TcpStream::connect(addr).await?;
        socket.set_nodelay(self.nodelay)?;
        Ok(TcpSocket {
            socket,
            size: None,
            buf: BytesMut::new(),
        })
    }
}

pub struct TcpSocket {
    socket: TcpStream,
    size: Option<usize>,
    buf: BytesMut,
}

impl_async_stream!(TcpSocket, socket);
impl_anti_sticky!(TcpSocket, TcpStream);