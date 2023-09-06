use std::{net::SocketAddr, pin::Pin, task::{Context, self}};

use s2n_quic::{
    Server,
    Connection,
    stream::BidirectionalStream, client::Connect, Client,
};
use tokio::io::{AsyncRead, AsyncWrite, self};

use super::{Factory, SimpleRead, SimpleWrite, Safe, SimpleStream};

use fast_async_mutex::RwLock;

#[derive(Default, Debug)]
pub struct QuicFactory {
    pub crt: Option<String>,
    pub key: Option<String>,
    pub server_name: Option<String>
}

impl Safe for QuicFactory {
    
}

impl QuicFactory {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn set_server_name(&mut self, server_name: String) {
        self.server_name = Some(server_name)
    }

    pub fn set_crt(&mut self, crt: String) {
        self.crt = Some(crt);
    }
    
    pub fn set_key(&mut self, key: String) {
        self.key = Some(key)
    }
}

#[async_trait::async_trait]
impl Factory for QuicFactory {
    type Socket = QuicSocket;
    type Listener = QuicListner;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Listener> {
        let crt = self.crt.as_ref().unwrap().as_str();
        let key = self.key.as_ref().unwrap().as_str();
        let server = Server::builder()
        .with_tls((crt, key))?
        .with_io(addr)?
        .start()?;
        Ok(QuicListner { server: RwLock::new(server) })
    }
    async fn accept(&self, listener: &Self::Listener) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        let mut write_lock = listener.server.write().await;
        let Some(mut connection) = write_lock.accept().await else {
            return Err(anyhow::anyhow!("Can't accept connection"));
        };
        let Ok(Some(socket)) = connection.accept_bidirectional_stream().await else {
            return Err(anyhow::anyhow!("Can't accept stream"));
        };

        let addr = connection.remote_addr()?;
        Ok((QuicSocket {
            socket,
            connection
        }, addr))
    }
    async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        let crt = self.crt.as_ref().unwrap().as_str();
        let client = Client::builder()
            .with_tls(crt)?
            .with_io("0.0.0.0:0")?
            .start()?;
        let connect = Connect::new(addr).with_server_name(self.server_name.as_ref().unwrap().as_str());
        let mut connection = client.connect(connect).await?;
        connection.keep_alive(true)?;
        let socket = connection.open_bidirectional_stream().await?;
        Ok(QuicSocket { connection, socket })
    }
}

pub struct QuicListner {
    server: RwLock<Server>,
}

impl Safe for QuicListner {
    
}

pub struct QuicSocket {
    connection: Connection,
    socket: BidirectionalStream
}

impl SimpleStream for QuicSocket {
    
}

impl Safe for QuicSocket {
    
}

impl AsyncRead for QuicSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> task::Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.get_mut().socket), cx, buf)
    }
}

impl AsyncWrite for QuicSocket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> task::Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.get_mut().socket), cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> task::Poll<io::Result<()>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().socket), cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> task::Poll<io::Result<()>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.get_mut().socket), cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> task::Poll<io::Result<usize>> {
        AsyncWrite::poll_write_vectored(Pin::new(&mut self.get_mut().socket), cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        AsyncWrite::is_write_vectored(&self.socket)
    }
}

#[async_trait::async_trait]
impl SimpleRead for QuicSocket {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::with_capacity(4096);
        let size = self.read_buf(&mut buf).await?;
        Ok(buf[..size].to_vec())
    }
}

#[async_trait::async_trait]
impl SimpleWrite for QuicSocket {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        AsyncWriteExt::write(self, data).await?;
        self.flush().await?;
        Ok(())
    }
}