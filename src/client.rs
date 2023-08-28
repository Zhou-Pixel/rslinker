use crate::protocol::{
    Accepted, BasicProtocol, ClientToServer, Port, Protocol, ServerToClient, SimpleRead,
    SimpleWrite,
};
use std::{collections::HashMap, net::SocketAddr};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, UdpSocket},
};

pub struct Client<T>
where
    T: Protocol,
{
    addr: SocketAddr,
    socket: T::Socket,
    id: Option<i64>,
    links: HashMap<Port, u16>,
    // tcp_links: HashMap<u16, u16>,
    // udp_links: HashMap<u16, u16>,
    accept_conflict: bool,
    // _marker: PhantomData<T>
}

impl<T: Protocol> Client<T> {
    pub fn set_links(&mut self, links: HashMap<Port, u16>) {
        self.links = links;
    }

    pub fn set_accept_conflict(&mut self, accept_conflict: bool) {
        self.accept_conflict = accept_conflict;
    }

    pub async fn connect(addr: SocketAddr) -> anyhow::Result<Self> {
        log::info!("Try to connect to server: {addr}");
        let socket = T::connect(addr).await?;
        log::info!("Successfully connected to the server: {addr}");
        Ok(Self {
            addr,
            socket,
            id: None,
            links: Default::default(),
            // tcp_links: Default::default(),
            // udp_links: Default::default(),
            accept_conflict: false,
        })
    }

    async fn new_client(&mut self) -> anyhow::Result<()> {
        let msg = ClientToServer::NewClient;
        let json = serde_json::to_vec(&msg)?;
        self.socket.write(&json).await?;

        let msg = self.socket.read().await?;
        let msg = serde_json::from_slice::<ServerToClient>(&msg)?;
        if let ServerToClient::AcceptClient { id } = msg {
            self.id = Some(id);
            log::info!("The server accepted the client, id: {id}");
            anyhow::Ok(())
        } else {
            log::info!("Server rejected client");
            Err(anyhow::anyhow!("incorret msg: {:?}", msg))
        }
    }

    async fn push_config(&mut self) -> anyhow::Result<()> {
        let config: Vec<Port> = self.links.iter().map(|(k, _)| *k).collect();
        let msg = ClientToServer::PushConfig(config);
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;

        let msg = self.socket.read().await?;
        let msg = serde_json::from_slice::<ServerToClient>(&msg)?;

        let id = self.id.unwrap();
        match msg {
            ServerToClient::AcceptConfig(Accepted::All) => {
                log::info!("The server accepts all configuration, id: {id}");
                anyhow::Ok(())
            },
            ServerToClient::AcceptConfig(Accepted::Part(ports)) if self.accept_conflict => {
                log::info!("The server only accepted a partial configuration, id: {id}");
                self.links.retain(|k, _| ports.contains(&k));

                anyhow::Ok(())
            }
            _ => {
                log::info!("Push configuration failed, id: {id}");
                Err(anyhow::anyhow!("push config error: {:?}", msg))
            }
        }
    }

    pub async fn exec(&mut self) -> anyhow::Result<()> {
        self.new_client().await?;
        self.push_config().await?;
        loop {
            let msg = self.socket.read().await?;
            let msg = serde_json::from_slice::<ServerToClient>(&msg)?;
            match msg {
                ServerToClient::NewChannel {
                    port,
                    random_number,
                } => {
                    self.accept_channel(port, random_number);
                }
                _ => {
                    todo!()
                }
            }
        }
    }

    fn accept_channel(&self, port: Port, number: i64) {
        let id = self.id.unwrap();
        let local_port = match self.links.get(&port) {
            Some(p) => *p,
            None => return,
        };

        let addr = self.addr;
        tokio::spawn(async move {
            if let BasicProtocol::Tcp = port.protocol {
                let local_addr: SocketAddr = format!("127.0.0.1:{}", local_port).parse().unwrap();
                let mut local_socket = TcpStream::connect(local_addr).await?;

                let mut socket = T::connect(addr).await?;
                let msg = ClientToServer::AcceptChannel {
                    from: id,
                    port,
                    number,
                };
                let msg = serde_json::to_vec(&msg)?;
                socket.write(&msg).await?;

                tokio::io::copy_bidirectional(&mut socket, &mut local_socket).await?;
            } else {
                let local_addr: SocketAddr = format!("127.0.0.1:{}", local_port).parse().unwrap();
                let udp_socket = UdpSocket::bind("127.0.0.1:0").await?;
                udp_socket.connect(local_addr).await?;

                let mut socket = T::connect(addr).await?;
                let msg = ClientToServer::AcceptChannel {
                    from: id,
                    port,
                    number,
                };
                let msg = serde_json::to_vec(&msg)?;
                socket.write(&msg).await?;

                copy(&mut socket, &udp_socket).await?;
            }
            anyhow::Ok(())
        });
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            self.exec().await?;
            anyhow::Ok(())
        });
    }
}

async fn copy<T>(stream: &mut T, socket: &UdpSocket) -> anyhow::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + 'static,
{
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    loop {
        let mut stream_buf = Vec::new();
        let mut socket_buf = vec![];
        tokio::select! {
            result = stream.read_buf(&mut stream_buf) => {
                let size = result?;
                socket.send(&stream_buf[..size]).await?;
            },
            result = socket.recv_buf(&mut socket_buf) => {
                let size = result?;
                stream.write_all(&socket_buf[..size]).await?;
            }
        }
    }
}
