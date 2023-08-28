use crypto2::blockcipher::Aes256;
use once_cell::sync::Lazy;
use std::task::Context;
use std::{any::Any, collections::HashMap, net::SocketAddr, pin::Pin, task::Poll};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use fast_async_mutex::RwLock;

use super::{Options, Safe, SimpleRead, SimpleStream, SimpleWrite};

pub struct TcpProtocol {
    listener: TcpListener,
}

pub struct TcpOptions {
    cipher: Option<Aes256>,
    encrypt_all: bool
}


impl TcpOptions {
    pub fn instance() -> &'static RwLock<TcpOptions> {
        static SELF: Lazy<RwLock<TcpOptions>> = Lazy::new(||{
            RwLock::new(TcpOptions { cipher: None, encrypt_all: false })
        });
        &SELF
    }
    pub fn set_encrypt_all(&mut self, encrypt_all: bool) {
        self.encrypt_all = encrypt_all;
    }

    pub fn set_cipher(&mut self, cipher: Option<Aes256>) {
        self.cipher = cipher;
    }

    pub fn encrypt_bytes(&self, bytes: &mut [u8]) {
        if let Some(ref cipher) = self.cipher {
            cipher.encrypt(bytes);
        }
    }

    pub fn decrypt_bytes(&self, bytes: &mut [u8]) {
        if let Some(ref cipher) = self.cipher {
            cipher.decrypt(bytes);
        }
    }
}


impl Safe for TcpProtocol {}

#[async_trait::async_trait]
impl super::Protocol for TcpProtocol {
    type Socket = Socket;
    async fn bind(addr: SocketAddr) -> anyhow::Result<Self> {
        anyhow::Ok(TcpProtocol {
            listener: TcpListener::bind(addr).await?,
        })
    }
    async fn accept(&self) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;
        anyhow::Ok((Socket::new(stream), addr))
    }

    async fn connect(addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        anyhow::Ok(Socket::new(TcpStream::connect(addr).await?))
    }
}

impl Safe for Socket {}

pub struct Socket {
    stream: TcpStream,
}

impl Options for Socket {
    fn set_option(options: &HashMap<String, Box<dyn Any>>) {}
}


impl SimpleStream for Socket { }

#[async_trait::async_trait]
impl SimpleRead for Socket {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let size = self.read_u64_le().await? as u64;
        let mut buf = vec![0; size as usize];
        self.read_exact(&mut buf).await?;
        anyhow::Ok(buf)
    }
}

#[async_trait::async_trait]
impl SimpleWrite for Socket {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let size = data.len();
        self.write_u64_le(size as u64).await?;
        self.write_all(data).await?;
        anyhow::Ok(())
    }
}

impl AsyncRead for Socket {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.stream), cx, buf)
    }
}

impl AsyncWrite for Socket {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        AsyncWrite::poll_write(Pin::new(&mut self.stream), cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.stream), cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.stream), cx)
    }
}

impl Socket {
    // fn decrypt(&self, buf: &mut [u8]) {
    //     match self.cipher {
    //         Some(ref cipher) => cipher.decrypt(buf),
    //         None => {}
    //     };
    // }

    // fn encrypt(&mut self, buf: &mut [u8]) {
    //     match self.cipher {
    //         Some(ref cipher) => cipher.encrypt(buf),
    //         None => {}
    //     };
    // }

    fn new(stream: TcpStream) -> Self {
        Socket {
            stream,
        }
    }
}

// #[async_trait::async_trait]
// impl super::SimpleRead for Socket {}

// #[async_trait::async_trait]
// impl super::SimpleWrite for Socket {}

// #[async_trait::async_trait]
// impl super::SimpleRead for Socket {
//     async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
//         let mut buf = Vec::new();
//         self.stream.read_to_end(&mut buf).await?;
//         self.decrypt(&mut buf);
//         anyhow::Ok(buf)
//     }
// }

// #[async_trait::async_trait]
// impl super::SimpleWrite for Socket {
//     async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
//         let mut buf = data.to_vec();
//         self.encrypt(&mut buf);
//         anyhow::Ok(self.stream.write_all(&buf).await?)
//     }
// }

// #[async_trait::async_trait]
// impl crate::SimpleRead for TcpStream {
//     async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
//         let mut buf = Vec::new();
//         AsyncReadExt::read(self, &mut buf).await?;
//         anyhow::Ok(buf)
//     }

// }

// #[async_trait::async_trait]
// impl crate::SimpleWrite for TcpStream {
//     async fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
//         let size = AsyncWriteExt::write(self, data).await?;
//         anyhow::Ok(size)
//     }
// }
