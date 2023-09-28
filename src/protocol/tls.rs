use std::{
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{self, Context},
};

use super::{Factory, Verification, SimpleRead, SimpleWrite};
use bytes::BytesMut;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

use rustls::{server::AllowAnyAuthenticatedClient, Certificate, PrivateKey, RootCertStore};

// pub enum TlsFactory {
//     Server {
//         pair: (Vec<Certificate>, PrivateKey),
//         ca: Option<Certificate>,
//         enable_client_auth: bool
//     },
//     Client {
//         pair: Option<(Vec<Certificate>, PrivateKey)>,
//         server_name: String,
//         ca: Certificate,
//         enable_client_auth: bool
//     }
// }

// #[async_trait::async_trait]
// impl Factory for TlsFactory {
//     type Socket = TlsSocket;
//     type Listener = TlsListner;
//     async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Listener> {
//         match self {
//             TlsFactory::Server { pair, ca , enable_client_auth} => {
//                 let mut roots = pair.0.clone();
//                 let enable = match (*enable_client_auth, ca) {
//                     (true, Some(ca)) => {
//                         roots.push(ca.to_owned());
//                         true
//                     },
//                     (true, None) => {
//                         panic!("set ca first")
//                     }
//                     _ => false,
//                 };
//                 let config = rustls::ServerConfig::builder()
//                 .with_cipher_suites(rustls::ALL_CIPHER_SUITES)
//                 .with_safe_default_kx_groups()
//                 .with_protocol_versions(rustls::ALL_VERSIONS)?;

//                 let config = if enable {
//                     let mut client_auth_roots = RootCertStore::empty();
//                     for i in &roots {
//                         client_auth_roots.add(i)?;
//                     }
//                     let client_auth = AllowAnyAuthenticatedClient::new(client_auth_roots);
//                     config.with_client_cert_verifier(Arc::new(client_auth))
//                     .with_single_cert(roots, pair.1.to_owned())?
//                 } else {
//                     config.with_no_client_auth()
//                     .with_single_cert(roots.clone(), pair.1.to_owned())?
//                 };

//                 let acceptor = TlsAcceptor::from(Arc::new(config));
//                 let listner = TcpListener::bind(addr).await?;
//                 Ok(TlsListner { acceptor, listner })
//             },
//             TlsFactory::Client { .. } => unreachable!("Only Server can bind address"),
//         }
//     }

//     async fn accept(&self, listener: &Self::Listener) -> anyhow::Result<(Self::Socket, SocketAddr)> {
//         let (socket, addr) = listener.listner.accept().await?;
//         let tls = listener.acceptor.accept(socket).await?;

//         Ok((TlsSocket { socket: TlsStream::Server(tls) }, addr))

//     }

//     async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket> {

//         match self {
//             TlsFactory::Server { .. } => unreachable!("server can't"),
//             TlsFactory::Client { pair, server_name, ca, enable_client_auth } => {

//             },
//         }
//         todo!()
//     }
// }

#[derive(Debug, Default)]
pub struct TlsFactory {
    pub server_config: Option<TlsServerConfig>,
    pub client_config: Option<TlsClientConfig>,
}


impl TlsFactory {
    pub fn new() -> Self {
        Default::default()
    }
}


#[async_trait::async_trait]
impl Factory for TlsFactory {
    type Socket = TlsSocket;
    type Acceptor = TlsListner;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor> {
        let server_config = self
            .server_config
            .as_ref()
            .expect("server config not found");
        // match (config.enable_client_auth, config.) {

        // }
        let roots = server_config.verification.certs.clone();

        let config = rustls::ServerConfig::builder()
            .with_cipher_suites(rustls::ALL_CIPHER_SUITES)
            .with_safe_default_kx_groups()
            .with_protocol_versions(rustls::ALL_VERSIONS)?;

        let config = match (server_config.enable_client_auth, &server_config.ca) {
            (true, Some(ca)) => {
                let mut client_auth_roots = RootCertStore::empty();
                for i in &roots {
                    client_auth_roots.add(i)?;
                }
                client_auth_roots.add(ca)?;

                let client_auth = AllowAnyAuthenticatedClient::new(client_auth_roots);

                config
                    .with_client_cert_verifier(Arc::new(client_auth))
                    .with_single_cert(roots, server_config.verification.key.clone())?
            }
            (true, None) => {
                panic!("set ca first")
            }
            _ => config
                .with_no_client_auth()
                .with_single_cert(roots.clone(), server_config.verification.key.clone())?,
        };

        // let config = if enable {
        //     let mut client_auth_roots = RootCertStore::empty();
        //     for i in &roots {
        //         client_auth_roots.add(i)?;
        //     }
        //     let client_auth = AllowAnyAuthenticatedClient::new(client_auth_roots);
        //     config.with_client_cert_verifier(Arc::new(client_auth))
        //     .with_single_cert(roots, server_config.verification.key.clone())?
        // } else {
        //     config.with_no_client_auth()
        //     .with_single_cert(
        //     roots.clone(),
        //     server_config.verification.key.clone()
        //     )?
        // };

        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listner = TcpListener::bind(addr).await?;
        Ok(TlsListner { acceptor, listner })
    }

    async fn accept(
        &self,
        listener: &Self::Acceptor,
    ) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        let (socket, addr) = listener.listner.accept().await?;
        let tls = listener.acceptor.accept(socket).await?;

        Ok((
            TlsSocket {
                socket: TlsStream::Server(tls),
                size: None,
                buf: Default::default()
            },
            addr,
        ))
    }
    
    async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        let client_config = self
            .client_config
            .as_ref()
            .expect("Tls client config not found");
        let mut roots = RootCertStore::empty();

        if let Some(ref ca) = client_config.ca {
            roots.add(ca)?;
        }

        let config = rustls::ClientConfig::builder()
            .with_cipher_suites(rustls::DEFAULT_CIPHER_SUITES)
            .with_safe_default_kx_groups()
            .with_protocol_versions(rustls::ALL_VERSIONS)?
            .with_root_certificates(roots);

        let config = match (
            client_config.enable_client_auth,
            &client_config.verification,
        ) {
            (true, Some(verification)) => config
                .with_client_auth_cert(verification.certs.clone(), verification.key.clone())?,
            (true, None) => {
                panic!("set ca first")
            }
            _ => config.with_no_client_auth(),
        };

        let connector = TlsConnector::from(Arc::new(config));

        let domain = rustls::ServerName::try_from(client_config.server_name.as_str())?;

        let stream = TcpStream::connect(addr).await?;

        let socket = connector.connect(domain, stream).await?;

        let socket = TlsStream::Client(socket);

        Ok(TlsSocket { 
            socket,
            size: None,
            buf: BytesMut::default()
        })

        // Ok(
        //     TlsSocket {
        //         socket: TlsStream::Client(connector.connect(
        //         server_name,
        //         TcpStream::connect(addr).await?
        //     ).await?)
        //     }
        // )
    }
}


#[derive(Debug)]
pub struct TlsServerConfig {
    pub verification: Verification,
    pub ca: Option<Certificate>,
    pub enable_client_auth: bool,
}

#[derive(Debug)]
pub struct TlsClientConfig {
    pub verification: Option<Verification>,
    pub server_name: String,
    pub ca: Option<Certificate>,
    pub enable_client_auth: bool,
}

pub struct TlsListner {
    acceptor: TlsAcceptor,
    listner: TcpListener,
}

pub struct TlsSocket {
    socket: TlsStream<TcpStream>,
    size: Option<usize>,
    buf: BytesMut,
}

impl AsyncRead for TlsSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().socket).poll_read(cx, buf)
    }
}

impl AsyncWrite for TlsSocket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> task::Poll<Result<usize, std::io::Error>> {
        AsyncWrite::poll_write(Pin::new(&mut self.get_mut().socket), cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> task::Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().socket), cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().socket).poll_flush(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.socket.is_write_vectored()
    }
}

#[async_trait::async_trait]
impl SimpleWrite for TlsSocket {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let size = data.len();
        self.write_u64_le(size as u64).await?;
        self.write_all(data).await?;
        self.flush().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl SimpleRead for TlsSocket {
    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
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


impl TlsSocket {
    async fn read_size(&mut self) -> anyhow::Result<()> {
        use tokio::io::AsyncReadExt;
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
