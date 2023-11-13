macro_rules! impl_async_read {
    ($type:ty, $inner:tt) => {
        impl tokio::io::AsyncRead for $type {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                tokio::io::AsyncRead::poll_read(std::pin::Pin::new(&mut self.get_mut().$inner), cx, buf)
            }
        }
    };
    ($type:ty, $inner:tt, $t:tt, $($tr:ident),+) => {
        impl<$t: $($tr+)*> tokio::io::AsyncRead for $type {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                tokio::io::AsyncRead::poll_read(std::pin::Pin::new(&mut self.get_mut().$inner), cx, buf)
            }
        }
    }
}

macro_rules! impl_async_write {
    ($type:ty, $inner:tt) => {
        impl tokio::io::AsyncWrite for $type {
            fn poll_write(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                tokio::io::AsyncWrite::poll_write(std::pin::Pin::new(&mut self.get_mut().$inner), cx, buf)
            }
        
            fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
                tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(&mut self.get_mut().$inner), cx)
            }
        
            fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
                tokio::io::AsyncWrite::poll_shutdown(std::pin::Pin::new(&mut self.get_mut().$inner), cx)
            }
        
            fn is_write_vectored(&self) -> bool {
                tokio::io::AsyncWrite::is_write_vectored(&self.$inner)
            }
        
            fn poll_write_vectored(
                    self: std::pin::Pin<&mut Self>,
                    cx: &mut std::task::Context<'_>,
                    bufs: &[std::io::IoSlice<'_>],
                ) -> std::task::Poll<Result<usize, std::io::Error>> {
                tokio::io::AsyncWrite::poll_write_vectored(std::pin::Pin::new(&mut self.get_mut().$inner), cx, bufs)
            }
        }
    };

    ($type:ty, $inner:tt, $t:tt, $($tr:ident),+) => {
        impl<$t: $($tr+)*> tokio::io::AsyncWrite for $type {
            fn poll_write(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                tokio::io::AsyncWrite::poll_write(std::pin::Pin::new(&mut self.get_mut().$inner), cx, buf)
            }
        
            fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
                tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(&mut self.get_mut().$inner), cx)
            }
        
            fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
                tokio::io::AsyncWrite::poll_shutdown(std::pin::Pin::new(&mut self.get_mut().$inner), cx)
            }
        
            fn is_write_vectored(&self) -> bool {
                tokio::io::AsyncWrite::is_write_vectored(&self.$inner)
            }
        
            fn poll_write_vectored(
                    self: std::pin::Pin<&mut Self>,
                    cx: &mut std::task::Context<'_>,
                    bufs: &[std::io::IoSlice<'_>],
                ) -> std::task::Poll<Result<usize, std::io::Error>> {
                tokio::io::AsyncWrite::poll_write_vectored(std::pin::Pin::new(&mut self.get_mut().$inner), cx, bufs)
            }
        }
    };
}

macro_rules! impl_async_stream {
    ($type:ty, $inner:tt) => {
        impl_async_read!($type, $inner);
        impl_async_write!($type, $inner);
    };

    ($type:ty, $inner:tt, $t:tt, $($tr:ident),+) => {
        impl_async_read!($type, $inner, $t, $($tr),+);
        impl_async_write!($type, $inner, $t, $($tr),+);
    }
}

pub mod quic;
pub mod tcp;
pub mod tls;
pub mod udp;
pub mod kcp;

use byteorder::ReadBytesExt;
use bytes::BufMut;
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, str::FromStr};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use byteorder::WriteBytesExt;
use byteorder::LittleEndian;
use rustls::{Certificate, PrivateKey};


pub trait Safe: Send + Sync + 'static {}

impl<T> Safe for T where T: Send + Sync + 'static {}

#[derive(Debug)]
pub struct Verification {
    pub certs: Vec<Certificate>,
    pub key: PrivateKey,
}

#[async_trait::async_trait]
pub trait Factory: Sized + Safe {
    type Socket: SimpleStream + Safe;
    type Acceptor: Safe;
    type Connector: Safe;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor>;
    async fn accept(&self, acceptor: &Self::Acceptor)
        -> anyhow::Result<(Self::Socket, SocketAddr)>;
    async fn make(&self) -> anyhow::Result<Self::Connector>;
    async fn connect(
        &self,
        connector: &Self::Connector,
        addr: SocketAddr,
    ) -> anyhow::Result<Self::Socket>;
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Port {
    pub port: u16,
    pub protocol: BasicProtocol,
}

impl std::fmt::Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", self.protocol, self.port)
    }
}

#[derive(Debug, Clone)]
pub enum CacheAddr {
    SocketAddr(SocketAddr),
    Domain(String, SocketAddr),
}

impl std::fmt::Display for CacheAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheAddr::SocketAddr(addr) => write!(f, "{}", addr),
            CacheAddr::Domain(name, addr) => write!(f, "{}({})", name, addr),
        }
    }
}

impl CacheAddr {
    pub async fn resolve(address: &str) -> anyhow::Result<Self> {
        if let Ok(addr) = SocketAddr::from_str(address) {
            Ok(CacheAddr::SocketAddr(addr))
        } else {
            let addrs: Vec<_> = tokio::net::lookup_host(address).await?.collect();
            Ok(CacheAddr::Domain(
                address.to_string(),
                *addrs
                    .first()
                    .ok_or(anyhow::anyhow!("can't resolve address: {}", address))?,
            ))
        }
    }

    pub fn socketaddr(&self) -> SocketAddr {
        match self {
            CacheAddr::SocketAddr(addr) => *addr,
            CacheAddr::Domain(_, addr) => *addr,
        }
    }

    pub fn domain(&self) -> Option<&str> {
        match self {
            CacheAddr::SocketAddr(_) => None,
            CacheAddr::Domain(domain, _) => Some(&domain),
        }
    }
}

pub trait SimpleStream: SimpleRead + SimpleWrite + Unpin {}

impl<T> SimpleStream for T where T: SimpleRead + SimpleWrite + Unpin {}

#[async_trait::async_trait]
pub trait SimpleRead: AsyncRead + Unpin {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>>;
}

#[async_trait::async_trait]
pub trait SimpleWrite: AsyncWrite + Unpin {
    // async fn write(&mut self, data: &[u8]) -> anyhow::Result<usize>;
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()>;
}


pub struct AntiStickyStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin
{
    socket: T,
    w_buf: bytes::BytesMut,
    r_buf: bytes::BytesMut,
    size: Option<usize>
}

impl<T: AsyncWrite + AsyncRead + Unpin> AntiStickyStream<T> {
    async fn read_one(&mut self) -> anyhow::Result<Vec<u8>> {
        if self.size.is_none() {
            self.read_size().await?;
        }
        let size = self.size.unwrap();

        while self.r_buf.len() < size {
            self.socket.read_buf(&mut self.r_buf).await?;
        }
        self.size = None;
        Ok(self.r_buf.split_to(size).to_vec())
    }

    async fn read_size(&mut self) -> anyhow::Result<()> {
        let size = std::mem::size_of::<u64>();
        while self.r_buf.len() < size {
            self.socket.read_buf(&mut self.r_buf).await?;
        }

        let tmp = self.r_buf.split_to(size);

        let size: u64 = ReadBytesExt::read_u64::<LittleEndian>( &mut tmp.as_ref())?;
        self.size = Some(size as usize);
        Ok(())
    }

    async fn write_one(&mut self, data: &[u8]) -> anyhow::Result<()> {

        let size = data.len() as u64;
        let mut tmp = vec![];

        WriteBytesExt::write_u64::<LittleEndian>(&mut tmp, size)?;

        self.w_buf.put(&tmp[..]);
        self.w_buf.put(data);

        while self.w_buf.len() > 0 {
            self.socket.write_buf(&mut self.w_buf).await?;
        }
        self.socket.flush().await?;
        Ok(())
    }

    fn new(socket: T) -> Self {
        Self {
            socket,
            w_buf: BytesMut::new(),
            r_buf: BytesMut::new(),
            size: None
        }
    }


}

#[async_trait::async_trait]
impl<T> SimpleRead for AntiStickyStream<T> 
where
    T: AsyncRead + AsyncWrite + Unpin + Safe
{
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        self.read_one().await
    }
}

#[async_trait::async_trait]
impl<T> SimpleWrite for AntiStickyStream<T> 
where
    T: AsyncRead + AsyncWrite + Unpin + Safe
{
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.write_one(data).await
    }
}

impl_async_stream!(AntiStickyStream<T>, socket, T, AsyncWrite, Unpin, AsyncRead);

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BasicProtocol {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "udp")]
    Udp,
}

impl std::fmt::Display for BasicProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            BasicProtocol::Tcp => "tcp",
            BasicProtocol::Udp => "udp",
        };
        write!(f, "{str}")
    }
}

// pub enum ChannelMessage {
//     Cancel,
// }

// async fn wait<A, B, C: Clone>(shutdown: &mut Option<Employee<A, B, C>>) -> Option<A> {
//     match shutdown {
//         Some(ref mut employee) => employee.wait().await,
//         None => {
//             Never::never().await;
//             None
//         }
//     }
// }

// pub async fn cancelable_copy<A, B>(
//     first: &mut A,
//     second: &mut B,
//     mut shutdown: Option<Employee<ChannelMessage, ()>>,
// ) -> anyhow::Result<()>
// where
//     A: AsyncRead + AsyncWrite + Unpin,
//     B: AsyncRead + AsyncWrite + Unpin,
// {
//     // let mut second_buf = Vec::new();
//     loop {
//         let mut first_buf = Vec::new();
//         let mut second_buf = Vec::new();
//         tokio::select! {
//             result = first.read_buf(&mut first_buf) => {
//                 let size = result?;
//                 second.write_all(&first_buf[..size]).await?;
//             }
//             result = second.read_buf(&mut second_buf) => {
//                 let size = result?;
//                 first.write_all(&second_buf[..size]).await?;
//             }
//             _ = wait(&mut shutdown) => {
//                 break;
//             }
//         }
//     }
//     Ok(())
// }
