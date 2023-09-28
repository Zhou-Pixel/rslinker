pub mod quic;
pub mod tcp;
pub mod udp;
pub mod tls;

use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, collections::HashMap, any::Any, str::FromStr};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufStream},
    net::{TcpStream, UdpSocket},
};

use rustls::{Certificate, PrivateKey};

use crate::utils::{chat::Employee, Never};

pub trait Safe: Send + Sync + 'static { }

impl<T> Safe for T  
where
    T: Send + Sync + 'static
{
    
}

#[derive(Debug)]
pub struct Verification {
    pub certs: Vec<Certificate>,
    pub key: PrivateKey,
}

pub trait Options {
    fn set_option(options: &HashMap<String, Box<dyn Any>>);
}

#[async_trait::async_trait]
pub trait Accept {
    type Socket: SimpleStream + Safe;
    async fn accept(&self) -> anyhow::Result<Self::Socket>;
}

#[async_trait::async_trait]
pub trait Connect {
    type Socket: SimpleStream + Safe;   
    async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket>;
}

#[async_trait::async_trait]
pub trait FFactory: Sized + Safe {
    type Acceptor: Accept;
    type Connector: Connect;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor>;
    async fn make(&self) -> anyhow::Result<Self::Connector>;
}

#[async_trait::async_trait]
pub trait Factory: Sized + Safe {
    type Socket: SimpleStream + Safe;
    type Acceptor: Safe;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor>;
    async fn accept(&self, acceptor: &Self::Acceptor) -> anyhow::Result<(Self::Socket, SocketAddr)>;
    async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket>;
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Port {
    pub port: u16,
    pub protocol: BasicProtocol,
}


#[derive(Debug, Clone)]
pub enum Address {
    SocketAddr(SocketAddr),
    Hostname(String, SocketAddr)
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
                *addrs.first().ok_or(anyhow::anyhow!("can't no resolve address: {}", address))?
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





pub enum Command {
    Close,
}


// pub trait Stream: Unpin + AsyncRead + AsyncWrite { }

pub trait SimpleStream: SimpleRead  + SimpleWrite + Unpin { }

impl<T> SimpleStream for T 
where 
    T: SimpleRead + SimpleWrite + Unpin
{
    
}

// impl Stream for TcpStream { }



#[async_trait::async_trait]
pub trait SimpleRead: AsyncRead + Unpin {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        let size = self.read_u64_le().await? as u64;
        let mut buf = vec![0; size as usize];
        self.read_exact(&mut buf).await?;
        anyhow::Ok(buf)
    }
}

#[async_trait::async_trait]
pub trait SimpleWrite: AsyncWrite + Unpin {
    // async fn write(&mut self, data: &[u8]) -> anyhow::Result<usize>;
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let size = data.len();
        self.write_u64_le(size as u64).await?;
        self.write_all(data).await?; 
        anyhow::Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BasicProtocol {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "udp")]
    Udp,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum ReliableProtocol {
    #[serde(rename="tcp")]
    Tcp,

    #[serde(rename="quic")]
    Quic,

    #[serde(rename="tls")]
    Tls,
}

pub enum ChannelMessage {
    Cancel,
}

async fn wait<A, B, C: Clone>(shutdown: &mut Option<Employee<A, B, C>>) -> Option<A> {
    match shutdown {
        Some(ref mut employee) => employee.wait().await,
        None => {
            Never::never().await;
            None
        }
    }
}

pub async fn cancelable_copy<A, B>(
    first: &mut A,
    second: &mut B,
    mut shutdown: Option<Employee<ChannelMessage, ()>>,
) -> anyhow::Result<()>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    // let mut second_buf = Vec::new();
    loop {
        let mut first_buf = Vec::new();
        let mut second_buf = Vec::new();
        tokio::select! {
            result = first.read_buf(&mut first_buf) => {
                let size = result?;
                second.write_all(&first_buf[..size]).await?;
            }
            result = second.read_buf(&mut second_buf) => {
                let size = result?;
                first.write_all(&second_buf[..size]).await?;
            }
            _ = wait(&mut shutdown) => {
                break;
            }
        }
    }
    Ok(())
}

// #[async_trait::async_trait]
// pub trait SimpleSend {
//     async fn send(&mut self, data: &[u8]) -> anyhow::Result<()>;
// }

// #[async_trait::async_trait]
// pub trait SimpleRecv {
//     async fn recv(&mut self) -> anyhow::Result<Vec<u8>>;
// }

// pub async fn copy<T>(
//     channel1: &mut T,
//     channel2: &mut T,
//     mut shutdown: Option<Receiver<Command>>,
// ) -> anyhow::Result<()>
//     where T: SimpleRead + SimpleWrite
// {
//     match shutdown {
//         Some(ref mut shutdown) => loop {
//             tokio::select! {
//                 ret = channel1.read() => {
//                     let vec = ret?;
//                     channel2.write(vec.as_slice()).await?;
//                 },
//                 ret = channel2.read() => {
//                     let vec = ret?;
//                     channel1.write(vec.as_slice()).await?;
//                 }
//                 _ = shutdown.recv() => {
//                     break;
//                 }
//             }
//         },
//         None => loop {
//             tokio::select! {
//                 ret = channel1.read() => {
//                     let vec = ret?;
//                     channel2.write(vec.as_slice()).await?;
//                 },
//                 ret = channel2.read() => {
//                     let vec = ret?;
//                     channel1.write(vec.as_slice()).await?;
//                 }
//             }
//         },
//     }

//     anyhow::Ok(())
// }

pub enum LocalSocket<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    Udp(UdpSocket),
    Tcp(T),
}

impl<T> LocalSocket<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    pub async fn connect_udp(addr: SocketAddr) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        socket.connect(addr).await?;
        anyhow::Ok(LocalSocket::Udp(socket))
    }

    pub fn new_tcp(tcp: T) -> Self {
        LocalSocket::Tcp(tcp)
    }

    pub async fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
        let size = match self {
            LocalSocket::Udp(ref udp) => udp.send(data).await?,
            LocalSocket::Tcp(ref mut tcp) => {
                tcp.write_all(data).await?;
                data.len()
            }
        };
        anyhow::Ok(size)
    }

    pub async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(4096 * 10);
        let size = match self {
            LocalSocket::Udp(ref udp) => udp.recv_buf(&mut buf).await?,
            LocalSocket::Tcp(ref mut tcp) => tcp.read_to_end(&mut buf).await?,
        };
        anyhow::Ok(buf[..size].to_vec())
    }
}

impl LocalSocket<TcpStream> {
    pub async fn connect_tcp_stream(addr: SocketAddr) -> anyhow::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        anyhow::Ok(LocalSocket::Tcp(stream))
    }
}

impl LocalSocket<BufStream<TcpStream>> {
    pub async fn connect_tcp_buf_stream(addr: SocketAddr) -> anyhow::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let streamer = BufStream::new(stream);
        anyhow::Ok(LocalSocket::Tcp(streamer))
    }
}

// pub async fn copy<T, S>(
//     channel1: &mut T,
//     channel2: &mut LocalSocket<S>,
//     mut shutdown: Option<Receiver<Command>>,
// ) -> anyhow::Result<()>
// where
//     T: SimpleRead + SimpleWrite,
//     S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
// {
//     match shutdown {
//         Some(ref mut shutdown) => loop {
//             tokio::select! {
//                 ret = channel1.read() => {
//                     let vec = ret?;
//                     channel2.write(vec.as_slice()).await?;
//                 },
//                 ret = channel2.read() => {
//                     let vec = ret?;
//                     channel1.write(&vec).await?;
//                 }
//                 _ = shutdown.recv() => {
//                     break;
//                 }
//             }
//         },
//         None => loop {
//             tokio::select! {
//                 ret = channel1.read() => {
//                     let vec = ret?;
//                     channel2.write(vec.as_slice()).await?;
//                 },
//                 ret = channel2.read() => {
//                     let vec = ret?;
//                     channel1.write(&vec).await?;
//                 }
//             }
//         },
//     }
//     anyhow::Ok(())
// }
