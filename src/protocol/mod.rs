pub mod quic;
pub mod tcp;
pub mod udp;
pub mod tls;

use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, collections::HashMap, any::Any, str::FromStr};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

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
pub trait Factory: Sized + Safe {
    type Socket: SimpleStream + Safe;
    type Acceptor: Safe;
    type Connector: Safe;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor>;
    async fn accept(&self, acceptor: &Self::Acceptor) -> anyhow::Result<(Self::Socket, SocketAddr)>;
    async fn make(&self) -> anyhow::Result<Self::Connector>;
    async fn connect(&self, connector: &Self::Connector, addr: SocketAddr) -> anyhow::Result<Self::Socket>;
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

pub trait SimpleStream: SimpleRead  + SimpleWrite + Unpin { }

impl<T> SimpleStream for T 
where 
    T: SimpleRead + SimpleWrite + Unpin
{
    
}


#[async_trait::async_trait]
pub trait SimpleRead: AsyncRead + Unpin {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>>;
}

#[async_trait::async_trait]
pub trait SimpleWrite: AsyncWrite + Unpin {
    // async fn write(&mut self, data: &[u8]) -> anyhow::Result<usize>;
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()>;
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

