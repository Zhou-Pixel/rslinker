use std::{
    net::SocketAddr,
    sync::Arc,
};

use super::{Factory, Verification};
use bytes::BytesMut;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

use rustls::{server::AllowAnyAuthenticatedClient, Certificate, RootCertStore};


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
    type Acceptor = TlsListener;
    type Connector = TlsLinker;
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


        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind(addr).await?;
        Ok(TlsListener { acceptor, listener })
    }

    async fn accept(
        &self,
        listener: &Self::Acceptor,
    ) -> anyhow::Result<(Self::Socket, SocketAddr)> {
        log::info!("Tls: accept ");
        let (socket, addr) = listener.listener.accept().await?;
        log::info!("Tls: new connection {addr}");
        let tls = listener.acceptor.accept(socket).await?;

        Ok((
            TlsSocket {
                socket: TlsStream::Server(tls),
                size: None,
                buf: Default::default(),
            },
            addr,
        ))
    }

    async fn make(&self) -> anyhow::Result<Self::Connector> {
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

        Ok(TlsLinker {
            connector,
            server_name: client_config.server_name.clone(),
        })
    }

    async fn connect(
        &self,
        connector: &Self::Connector,
        addr: SocketAddr,
    ) -> anyhow::Result<Self::Socket> {
        let domain = rustls::ServerName::try_from(connector.server_name.as_str())?;

        let stream = TcpStream::connect(addr).await?;

        let socket = connector.connector.connect(domain, stream).await?;

        let socket = TlsStream::Client(socket);

        Ok(TlsSocket {
            socket,
            size: None,
            buf: BytesMut::default(),
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

pub struct TlsListener {
    acceptor: TlsAcceptor,
    listener: TcpListener,
}

pub struct TlsLinker {
    connector: TlsConnector,
    server_name: String,
}

pub struct TlsSocket {
    socket: TlsStream<TcpStream>,
    size: Option<usize>,
    buf: BytesMut,
}

impl_async_stream!(TlsSocket, socket);

impl_anti_sticky!(TlsSocket, TlsStream<TcpStream>);
