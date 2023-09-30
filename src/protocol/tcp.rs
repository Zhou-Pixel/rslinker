use std::{net::SocketAddr, pin::Pin, task::{Poll, Context}, io::{IoSlice, self}};
use tokio::{net::{TcpListener, TcpStream}, io::{AsyncReadExt, AsyncWriteExt}};

use super::{SimpleRead, SimpleWrite};

use tokio::io::{AsyncWrite, AsyncRead};
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

    async fn accept(&self, listener: &Self::Acceptor) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        let (socket, addr) = listener.accept().await?;
        socket.set_nodelay(self.nodelay)?;
        Ok((TcpSocket {
           socket,
           size: None,
           buf: BytesMut::new()
        }, addr))
    }


    async fn make(&self) -> anyhow::Result<Self::Connector> {
        Ok(())
    }


    async fn connect(&self, _: &(), addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        let socket = TcpStream::connect(addr).await?;
        log::info!("connected!: {addr} {socket:?} {:?}", socket.peer_addr());
        socket.set_nodelay(self.nodelay)?;
        Ok(TcpSocket {
            socket,
            size: None,
            buf: BytesMut::new()
        })
    }
}


#[async_trait::async_trait]
impl SimpleRead for TcpStream {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let size = self.read_u64_le().await? as usize;
        log::info!("read number end");
        let mut buf = vec![0; size];
        self.read_exact(&mut buf).await?;
        Ok(buf)
    }
}

#[async_trait::async_trait]
impl SimpleWrite for TcpStream {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let size = data.len();
        self.write_u64_le(size as u64).await?;
        self.write_all(data).await?;
        self.flush().await?;
        Ok(())
    }
}
pub struct TcpSocket {
    socket: TcpStream,
    size: Option<usize>,
    buf: BytesMut
}

impl AsyncWrite for TcpSocket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        AsyncWrite::poll_write(Pin::new(&mut self.get_mut().socket), cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().socket), cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().socket), cx)
    }

    fn is_write_vectored(&self) -> bool {
        AsyncWrite::is_write_vectored(&self.socket)
    }
    fn poll_write_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<Result<usize, io::Error>> {
        AsyncWrite::poll_write_vectored(Pin::new(&mut self.get_mut().socket), cx, bufs)
    }
}


impl AsyncRead for TcpSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.get_mut().socket), cx, buf)
    }
}

impl TcpSocket {
    
    async fn read_size(&mut self) -> anyhow::Result<()> {
        fn is_little_endian() -> bool {
            union Check {
                l: u16,
                s: [u8; 2],
            }
            unsafe {
                let c = Check { l: 0x0102 };
                return c.s[0] == 2;
            }
        }


        let u64size = std::mem::size_of::<u64>();
        while self.buf.len() < u64size {
            self.socket.read_buf(&mut self.buf).await?;
        }

        let mut tmp = self.buf.split_to(u64size);
        if !is_little_endian() {
            tmp.reverse();
        }

        self.size = Some(unsafe {std::ptr::read(tmp.as_ptr() as *const _)});
        Ok(())
    }
}

#[async_trait::async_trait]
impl SimpleRead for TcpSocket {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        let size = if let Some(size) = self.size {
            size - self.buf.len()
        } else {
            self.read_size().await?;
            self.size.unwrap()
        };

        while self.buf.len() < size {
            self.socket.read_buf(&mut self.buf).await?;
        }
        self.size = None;
        Ok(std::mem::take(&mut self.buf).to_vec())
    }
}

#[async_trait::async_trait]
impl SimpleWrite for TcpSocket {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let size = data.len();
        self.write_u64_le(size as u64).await?;
        self.write_all(data).await?;
        self.socket.flush().await?;
        Ok(())
    }
}



