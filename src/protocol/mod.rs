macro_rules! impl_anti_sticky {
    ($x:ty, $y:ty) => {
        impl crate::protocol::AntiSticky for $x {
            type Socket = $y;
            fn get_mut(&mut self) -> (&mut Self::Socket, &mut Option<usize>, &mut bytes::BytesMut) {
                (&mut self.socket, &mut self.size, &mut self.buf)
            }
        }
    };
}

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
}

macro_rules! impl_async_stream {
    ($type:ty, $inner:tt) => {
        impl_async_read!($type, $inner);
        impl_async_write!($type, $inner);
    };
}

pub mod quic;
pub mod tcp;
pub mod tls;
pub mod udp;
pub mod kcp;

use serde::{Deserialize, Serialize};
use std::{any::Any, collections::HashMap, net::SocketAddr, str::FromStr};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use rustls::{Certificate, PrivateKey};


pub trait Safe: Send + Sync + 'static {}

impl<T> Safe for T where T: Send + Sync + 'static {}

#[derive(Debug)]
pub struct Verification {
    pub certs: Vec<Certificate>,
    pub key: PrivateKey,
}

pub trait Options {
    fn set_option(options: &HashMap<String, Box<dyn Any>>);
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
pub enum Address {
    SocketAddr(SocketAddr),
    Hostname(String, SocketAddr),
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Address::SocketAddr(addr) => write!(f, "{}", addr),
            Address::Hostname(name, addr) => write!(f, "{}({})", name, addr),
        }
    }
}

impl Address {
    pub async fn resolve(address: &str) -> anyhow::Result<Self> {
        if let Ok(addr) = SocketAddr::from_str(address) {
            Ok(Address::SocketAddr(addr))
        } else {
            let addrs: Vec<_> = tokio::net::lookup_host(address).await?.collect();
            Ok(Address::Hostname(
                address.to_string(),
                *addrs
                    .first()
                    .ok_or(anyhow::anyhow!("can't no resolve address: {}", address))?,
            ))
        }
    }

    pub fn socketaddr(&self) -> SocketAddr {
        match self {
            Address::SocketAddr(addr) => *addr,
            Address::Hostname(_, addr) => *addr,
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


#[async_trait::async_trait]
pub trait AntiSticky {
    type Socket: AsyncRead + AsyncWrite + Safe + Unpin;
    fn get_mut(&mut self) -> (&mut Self::Socket, &mut Option<usize>, &mut bytes::BytesMut);

    // This method is cancellation safe.
    async fn read_size(&mut self) -> anyhow::Result<()> {
        use tokio::io::AsyncReadExt;

        let (socket, size, buf) = self.get_mut();
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
        while buf.len() < u64size {
            socket.read_buf(buf).await?;
        }

        let mut tmp = buf.split_to(u64size);
        if !is_little_endian() {
            tmp.reverse();
        }
        let size_u64: u64 = unsafe { std::ptr::read(tmp.as_ptr() as *const _) };

        *size = Some(size_u64 as usize);

        Ok(())

    }

    // This method is cancellation safe.
    async fn read_one(&mut self) -> anyhow::Result<Vec<u8>> {
        if self.is_size_none() {
            self.read_size().await?;
        }
        let (socket, size, buf) = self.get_mut();
        let size_usize = size.unwrap();
        while buf.len() < size_usize {
            socket.read_buf(buf).await?;
        }
        *size = None;
        Ok(buf.split_to(size_usize).to_vec())
        
    }

    // This method is not cancellation safe.
    async fn write_one(&mut self, data: &[u8]) -> anyhow::Result<()> {

        let size = data.len();
        let (socket, ..) = self.get_mut();
        socket.write_u64_le(size as u64).await?;
        socket.write_all(data).await?;
        socket.flush().await?;

        Ok(())
    }

    fn is_size_none(&mut self) -> bool {
        self.get_mut().1.is_none()
    }
}

#[async_trait::async_trait]
impl<T> SimpleRead for T 
where
    T: AntiSticky + Unpin + AsyncRead + Safe
{
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        self.read_one().await
    }
}


#[async_trait::async_trait]
impl<T> SimpleWrite for T 
where
    T: AntiSticky + Unpin + AsyncWrite + Safe
{
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.write_one(data).await
    }
}

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
