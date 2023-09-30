use std::{
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{self, Context}, time::Duration,
};

use tokio::io::{self, AsyncRead, AsyncWrite};

use super::{SimpleRead, SimpleWrite, Verification};
use bytes::BytesMut;
use rustls::Certificate;

#[derive(Default, Debug)]
pub struct QuicFactory {
    pub server_config: Option<QuicServerConfig>,
    pub client_config: Option<QuicClientConfig>,
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

impl QuicFactory {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait::async_trait]
impl super::Factory for QuicFactory {
    type Socket = QuicSocket;
    type Acceptor = QuicListner;
    type Connector = QuicConnector;
    async fn bind(&self, addr: SocketAddr) -> anyhow::Result<Self::Acceptor> {
        let server_config = self
            .server_config
            .as_ref()
            .ok_or(anyhow::anyhow!("Quic server config must be set"))?;

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

                let client_auth =
                    rustls::server::AllowAnyAuthenticatedClient::new(client_auth_roots);

                config
                    .with_client_cert_verifier(Arc::new(client_auth))
                    .with_single_cert(roots, server_config.verification.key.clone())?
            }
            (true, None) => {
                anyhow::bail!("Client certs must be set");
            }
            _ => config
                .with_no_client_auth()
                .with_single_cert(roots.clone(), server_config.verification.key.clone())?,
        };


        let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
        
        let mut transport = quinn::TransportConfig::default();

        // transport.max_idle_timeout(Some(Duration::from_secs(60).try_into().unwrap()))
                // .max_concurrent_bidi_streams(u8::MAX.into())
                // .max_concurrent_uni_streams(u32::MAX.into())
        transport.keep_alive_interval(Some(Duration::from_secs(3)));

        config.transport_config(Arc::new(transport));

        let endpoint = quinn::Endpoint::server(config, addr)?;

        Ok(QuicListner { endpoint })

    }

    async fn accept(
        &self,
        acceptor: &Self::Acceptor,
    ) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        log::info!("Quic accept");
        let connection = acceptor.endpoint.accept().await.ok_or(anyhow::anyhow!("Endpoint is closed"))?;
        let connection = connection.await?;
        
        log::info!("New quic connnection");
        let addr = connection.remote_address();
        let (sender, recver) = connection.accept_bi().await?;
        Ok((
            QuicSocket {
                connection,
                sender,
                recver,
                size: None,
                buf: BytesMut::new(),
            },
            addr,
        ))
    }

    async fn make(&self) -> anyhow::Result<Self::Connector> {
        let client_config = self
            .client_config
            .as_ref()
            .ok_or(anyhow::anyhow!("Quic config must be set"))?;

        let mut roots = rustls::RootCertStore::empty();

        if let Some(ref ca) = client_config.ca {
            roots.add(ca)?;
        }

        let config = rustls::ClientConfig::builder()
            .with_cipher_suites(rustls::ALL_CIPHER_SUITES)
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
                anyhow::bail!("Client certs is't set");
            }
            _ => config.with_no_client_auth(),
        };

        let mut config = quinn::ClientConfig::new(Arc::new(crypto));

        let mut transport = quinn::TransportConfig::default();

        // transport.max_idle_timeout(Some(Duration::from_secs(60).try_into().unwrap()))
        //         .max_concurrent_bidi_streams(u8::MAX.into())
                // .max_concurrent_uni_streams(u32::MAX.into())
        transport.keep_alive_interval(Some(Duration::from_secs(3)));

        config.transport_config(Arc::new(transport));

        let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        endpoint.set_default_client_config(config);

        Ok(QuicConnector {
            endpoint,
            server_name: client_config.server_name.clone(),
        })
    }

    async fn connect(
        &self,
        connector: &Self::Connector,
        addr: SocketAddr,
    ) -> anyhow::Result<Self::Socket> {
        let connection = connector
            .endpoint
            .connect(addr, &connector.server_name)?
            .await?;
        let (sender, recver) = connection.open_bi().await?;
        Ok(QuicSocket {
            connection,
            sender,
            recver,
            size: None,
            buf: BytesMut::new()
        })
    }
}

pub struct QuicListner {
    endpoint: quinn::Endpoint,
}

pub struct QuicConnector {
    endpoint: quinn::Endpoint,
    server_name: String,
}

pub struct QuicSocket {
    #[allow(dead_code)]
    connection: quinn::Connection,
    sender: quinn::SendStream,
    recver: quinn::RecvStream,
    size: Option<usize>,
    buf: BytesMut,
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

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> task::Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().sender), cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> task::Poll<Result<(), std::io::Error>> {
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
        use tokio::io::AsyncReadExt;
        let size = if let Some(size) = self.size {
            size
        } else {
            self.read_size().await?;
            self.size.unwrap()
        };
        
        while self.buf.len() < size {
            self.recver.read_buf(&mut self.buf).await?;
        }
        self.size = None;
        Ok(self.buf.split_to(size).to_vec())
    }
}

#[async_trait::async_trait]
impl SimpleWrite for QuicSocket {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let size = data.len();
        self.write_u64_le(size as u64).await?;
        self.write_all(data).await?;
        self.flush().await?;
        Ok(())
    }
}

impl QuicSocket {
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
            self.recver.read_buf(&mut self.buf).await?;
        }

        let mut tmp = self.buf.split_to(u64size);
        if !is_little_endian() {
            tmp.reverse();
        }
        let size: u64 = unsafe {std::ptr::read(tmp.as_ptr() as *const _)};

        self.size = Some(size as usize);
        Ok(())
    }
}
