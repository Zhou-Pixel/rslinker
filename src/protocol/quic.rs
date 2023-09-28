use std::{
    net::SocketAddr,
    pin::Pin,
    task::{self, Context},
};

use s2n_quic::{client::Connect, stream::BidirectionalStream, Client, Connection, Server};
use tokio::io::{self, AsyncRead, AsyncWrite};

use super::{Factory, SimpleRead, SimpleWrite};

use fast_async_mutex::RwLock;

mod quinn_factory {
    use std::{
        net::SocketAddr,
        pin::Pin,
        task::{self, Context}, time::Duration, sync::Arc,
    };

    use tokio::io::{self, AsyncRead, AsyncWrite, AsyncReadExt};

    use crate::utils;

    use super::{Factory, SimpleRead, SimpleWrite, super::Verification};

    use rustls::{Certificate, PrivateKey};


    pub struct ServerCertification {
        cert: Vec<Certificate>,
        key: PrivateKey,
    }

    pub struct ClientCertification {
        cert: Vec<Certificate>,
        server_name: String,
    }

    pub struct QuicFactory {
        pub server_config: Option<QuicServerConfig>,
        pub client_config: Option<QuicClientConfig>
    }
    
    #[derive(Debug)]
pub struct QuicServerConfig {
    pub verification: Verification,
    pub ca: Option<Certificate>,
    pub enable_client_auth: bool,
}

#[derive(Debug)]
pub struct QuicClientConfig {
    pub verification: Option<Verification>,
    pub server_name: String,
    pub ca: Option<Certificate>,
    pub enable_client_auth: bool,
}


    // #[async_trait::async_trait]
    // impl Factory for QuicFactory {
    //     type Socket: SimpleStream + Safe;
    //     type Listener: Safe;
    //     async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Listener>;
    //     async fn accept(&self, listener: &Self::Listener) -> anyhow::Result<(Self::Socket, SocketAddr)>;
    //     async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket>;
    // }


    #[async_trait::async_trait]
    impl super::Factory for QuicFactory {
        type Socket = QuicSocket;
        type Acceptor = QuicListner;

        async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor> {

            let server_config = self.server_config.as_ref().ok_or(anyhow::anyhow!("quic server config must be set"))?;

            let roots = server_config.verification.certs.clone();

            let config = rustls::ServerConfig::builder()
                .with_cipher_suites(rustls::ALL_CIPHER_SUITES)
                .with_safe_default_kx_groups()
                .with_protocol_versions(rustls::ALL_VERSIONS)?;
    
            let crypto = match (server_config.enable_client_auth, &server_config.ca) {
                (true, Some(ca)) => {
                    let mut client_auth_roots = rustls::RootCertStore::empty();
                    for i in &roots {
                        client_auth_roots.add(i)?;
                    }
                    client_auth_roots.add(ca)?;
    
                    let client_auth = rustls::server::AllowAnyAuthenticatedClient::new(client_auth_roots);
    
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

            let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
            let mut transport = quinn::TransportConfig::default();
            transport.max_concurrent_bidi_streams(0_u8.into());
            transport.max_concurrent_uni_streams(500_u32.into());
            config.transport_config(Arc::new(transport));
            
            let endpoint = quinn::Endpoint::server(config, addr)?;

            Ok(QuicListner {
                endpoint
            })

            // if let Some(ref config) = self.server {
            //     let mut server_config = quinn::ServerConfig::with_single_cert(
            //         config.cert.clone(), 
            //         config.key.clone()
            //     )?;
            //     let mut transport_config = quinn::TransportConfig::default();
            //     transport_config.keep_alive_interval(Some(Duration::from_secs(1)));
            //     server_config.transport = Arc::new(transport_config);
            //     let endpoint = quinn::Endpoint::server(server_config, addr)?;
            //     Ok(QuicListner {
            //         endpoint
            //     })
            // } else {
            //     Err(anyhow::anyhow!("Does not set server config"))
            // }
        }

        async fn accept(&self, listener: &Self::Acceptor) -> anyhow::Result<(Self::Socket, SocketAddr)> {
            let Some(connection) = listener.endpoint.accept().await else {
                anyhow::bail!("The endpoint is closed");
            };
            let connection = connection.await?;
            let addr = connection.remote_address();
            let (sender, recver) = connection.accept_bi().await?;
            Ok((QuicSocket {
                connection,
                sender,
                recver
            }, addr))
        }

        async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket> {

            let client_config = self.client_config.as_ref().ok_or(anyhow::anyhow!("quic config must be set"))?;

            let mut roots = rustls::RootCertStore::empty();

            if let Some(ref ca) = client_config.ca {
                roots.add(ca)?;
            }
    
            let config = rustls::ClientConfig::builder()
                .with_cipher_suites(rustls::DEFAULT_CIPHER_SUITES)
                .with_safe_default_kx_groups()
                .with_protocol_versions(rustls::ALL_VERSIONS)?
                .with_root_certificates(roots);
    
            let crypto = match (
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


            let mut config = quinn::ClientConfig::new(Arc::new(crypto));

            let mut transport = quinn::TransportConfig::default();

            transport.max_concurrent_bidi_streams(0u8.into());
            transport.max_concurrent_uni_streams(500u32.into());

            config.transport_config(Arc::new(transport));


            let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
            endpoint.set_default_client_config(config);

            let connection = endpoint.connect(addr, &client_config.server_name)?.await?;

            let (sender, recver) = connection.open_bi().await?;


            // if let Some(ref config) = self.client {
            //     let mut certs = rustls::RootCertStore::empty();
                
            //     for i in &config.cert {
            //         certs.add(i)?;
            //     }

            //     let bind_addr: SocketAddr = ([127, 0, 0, 1], 0).into();

            //     let mut endpoint = quinn::Endpoint::client(bind_addr)?;


            //     let client_config = quinn::ClientConfig::with_root_certificates(certs);

            //     endpoint.set_default_client_config(client_config);


            //     let connection = endpoint.connect(addr, &config.server_name)?.await?;


            //     let (sender, recver) = connection.open_bi().await?;

                Ok(QuicSocket {
                    connection,
                    sender,
                    recver
                })
            // } else {
                
            //     Err(anyhow::anyhow!("Does not set client config"))
            // }
            
        }
    }

    pub struct QuicListner {
        endpoint: quinn::Endpoint,
    }

    pub struct QuicSocket {
        connection: quinn::Connection,
        sender: quinn::SendStream,
        recver: quinn::RecvStream,
    }


    impl AsyncRead for QuicSocket {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> task::Poll<std::io::Result<()>> {
            AsyncRead::poll_read(Pin::new(&mut self.get_mut().recver), cx, buf)
        }
    }

    impl AsyncWrite for QuicSocket {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> task::Poll<Result<usize, std::io::Error>> {
            AsyncWrite::poll_write(Pin::new(&mut self.get_mut().sender), cx, buf)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> task::Poll<Result<(), std::io::Error>> {
            AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().sender), cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> task::Poll<Result<(), std::io::Error>> {
            AsyncWrite::poll_shutdown(Pin::new(&mut self.get_mut().sender), cx)
        }

        fn is_write_vectored(&self) -> bool {
            AsyncWrite::is_write_vectored(&self.sender)
        }

        fn poll_write_vectored(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                bufs: &[std::io::IoSlice<'_>],
            ) -> task::Poll<Result<usize, std::io::Error>> {
                AsyncWrite::poll_write_vectored(Pin::new(&mut self.get_mut().sender), cx, bufs)
        }
    }

    #[async_trait::async_trait]
    impl SimpleRead for QuicSocket {
        async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
            use tokio::io::AsyncRead;
            let mut buf = vec![];
            loop {
                let mut tmp = Vec::with_capacity(4096);
                let size = self.recver.read_buf(&mut tmp).await?;
                if size < 4096 {
                    break;
                }
            }
            anyhow::Ok(buf)
        }
    }

    #[async_trait::async_trait]
    impl SimpleWrite for QuicSocket {

    }

}

pub struct ClientCertification {
    pub crt: Vec<u8>,
    pub server_name: String,
}

pub struct ServerCertification {
    pub crt: Vec<u8>,
    pub key: Vec<u8>
}

#[derive(Default)]
pub struct QuicFactory {
    pub client: Option<ClientCertification>,
    pub server: Option<ServerCertification>,
}


impl QuicFactory {
    pub fn new() -> Self {
        Default::default()
    }

}

#[async_trait::async_trait]
impl Factory for QuicFactory {
    type Socket = QuicSocket;
    type Acceptor = QuicListner;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor> {
        if let Some(ref server_config) = self.server {
            let server = Server::builder()
            .with_tls((server_config.crt.as_slice(), server_config.key.as_slice()))?
            .with_io(addr)?
            .start()?;
            Ok(QuicListner { server: RwLock::new(server) })
        } else {
            Err(anyhow::anyhow!("Does not set server config"))
        }
    }
    async fn accept(
        &self,
        listener: &Self::Acceptor,
    ) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        let mut write_lock = listener.server.write().await;
        let Some(mut connection) = write_lock.accept().await else {
            anyhow::bail!("Can't accept connection");
        };
        let Ok(Some(socket)) = connection.accept_bidirectional_stream().await else {
            anyhow::bail!("Can't accept stream");
        };
        connection.keep_alive(true)?;
        let addr = connection.remote_addr()?;
        Ok((QuicSocket { socket, connection }, addr))
    }
    async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Self::Socket> {
        if let Some(ref client_config) = self.client {
            let client = Client::builder()
                .with_tls(client_config.crt.as_slice())?
                .with_io("0.0.0.0:0")?
                .start()?;
            let connect = Connect::new(addr)
            .with_server_name(client_config.server_name.as_str());
            let mut connection = client.connect(connect).await?;
            connection.keep_alive(true)?;
            let socket = connection.open_bidirectional_stream().await?;
            Ok(QuicSocket { connection, socket })
        } else {
            Err(anyhow::anyhow!("Does not set client config"))
        }
    }
}

pub struct QuicListner {
    server: RwLock<Server>,
}


pub struct QuicSocket {
    #[allow(dead_code)]
    connection: Connection,
    socket: BidirectionalStream,
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
