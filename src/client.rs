use crate::protocol::{Address, BasicProtocol, Factory, Port, SimpleRead, SimpleWrite};
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, UdpSocket},
};

use super::server::{self, Accepted};

#[derive(Serialize, Deserialize, Debug)]
pub enum JsonMessage {
    #[serde(rename = "new_client")]
    NewClient,

    #[serde(rename = "accept_channel")]
    AcceptChannel {
        from: u128,
        // server_port: u16,
        // client_port: u16,
        // protocol: BasicProtocol,
        port: Port,
        number: u128,
    },

    #[serde(rename = "push_config")]
    PushConfig {
        ports: Vec<Port>,
        heartbeat_interval: Option<u64>,
    },

    #[serde(rename = "heartbeat")]
    Heartbeat,
}

pub struct Client<T>
where
    T: Factory,
{
    addr: Address,
    socket: T::Socket,
    connector: Arc<T::Connector>,
    id: Option<u128>,
    links: HashMap<Port, SocketAddr>,
    accept_conflict: bool,
    factory: Arc<T>,
    heartbeat_interval: Option<u64>,
    retry_time: u32,
}

impl<T: Factory> Client<T> {
    pub fn set_links(&mut self, links: HashMap<Port, SocketAddr>) {
        self.links = links;
    }

    pub fn set_accept_conflict(&mut self, accept_conflict: bool) {
        self.accept_conflict = accept_conflict;
    }

    pub fn set_heartbeat_interval(&mut self, heartbeat_interval: Option<u64>) {
        self.heartbeat_interval = heartbeat_interval;
    }

    pub fn set_retry_times(&mut self, retry_times: u32) {
        self.retry_time = retry_times;
    }

    pub async fn connect(addr: Address, factory: T) -> anyhow::Result<Self> {
        log::info!("Try to connect to server: {addr:?}");
        let connector = factory.make().await?;
        let socket = factory.connect(&connector, addr.socketaddr()).await?;
        log::info!("Successfully connected to the server: {addr}");
        Ok(Self {
            addr,
            socket,
            connector: Arc::new(connector),
            id: None,
            links: Default::default(),
            // tcp_links: Default::default(),
            // udp_links: Default::default(),
            accept_conflict: false,
            factory: Arc::new(factory),
            heartbeat_interval: None,
            retry_time: 0,
        })
    }

    async fn new_client(&mut self) -> anyhow::Result<()> {
        let msg = JsonMessage::NewClient;
        let json = serde_json::to_vec(&msg)?;
        self.socket.write(&json).await?;

        let msg = self.socket.read().await?;
        let msg = serde_json::from_slice::<server::JsonMessage>(&msg)?;
        if let server::JsonMessage::AcceptClient { id } = msg {
            self.id = Some(id);
            log::info!("The server accepted the client, id: {id}");
            Ok(())
        } else {
            log::info!("Server rejected client");
            Err(anyhow::anyhow!("Incorret msg: {:?}", msg))
        }
    }

    async fn push_config(&mut self) -> anyhow::Result<()> {
        let config: Vec<Port> = self.links.iter().map(|(k, _)| *k).collect();
        let msg = JsonMessage::PushConfig {
            ports: config,
            heartbeat_interval: self.heartbeat_interval,
        };
        let msg = serde_json::to_vec(&msg)?;
        log::info!(
            "Start to push config: {}",
            String::from_utf8(msg.to_vec()).unwrap()
        );
        self.socket.write(&msg).await?;

        log::info!("Finish pushing config end");

        let msg = self.socket.read().await?;
        let msg = serde_json::from_slice::<server::JsonMessage>(&msg)?;

        let id = self.id.unwrap();
        match msg {
            server::JsonMessage::AcceptConfig(Accepted::All) => {
                log::info!("The server accepts all configuration, id: {id}");
                Ok(())
            }
            server::JsonMessage::AcceptConfig(Accepted::Part(ports)) if self.accept_conflict => {
                if ports.is_empty() && !self.links.is_empty() {
                    log::error!("No config was accepted");
                    Err(anyhow::anyhow!("No config was accepted"))
                } else {
                    log::info!("The server only accepted a partial configuration, id: {id}");
                    self.links.retain(|k, _| ports.contains(&k));

                    Ok(())
                }
            }
            _ => {
                log::error!("Push config failed, id: {id}");
                Err(anyhow::anyhow!("Push config failed: {:?}", msg))
            }
        }
    }

    async fn exec_with_heartbeat(&mut self, heartbeat_interval: u64) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(heartbeat_interval));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.send_heartbeat().await?;
                }
                result = self.socket.read() => {
                    let msg = result?;
                    if msg.len() == 0 {
                        log::info!("Disconnect!");
                        break;
                    }
                    self.recv_msg(msg)?;
                }
            }
        }
        anyhow::Ok(())
    }

    async fn exec_without_heartbeat(&mut self) -> anyhow::Result<()> {
        loop {
            let msg = self.socket.read().await?;
            if msg.len() == 0 {
                log::info!("Disconnect!");
                break;
            }
            self.recv_msg(msg)?;
        }
        anyhow::Ok(())
    }

    pub async fn exec(&mut self) -> anyhow::Result<()> {
        loop {
            let result = async {
                self.new_client().await?;
                self.push_config().await?;
                if let Some(heartbeat_interval) = self.heartbeat_interval {
                    self.exec_with_heartbeat(heartbeat_interval).await?;
                } else {
                    self.exec_without_heartbeat().await?;
                }
                anyhow::Ok(())
            }
            .await;

            match result {
                Ok(_) => {
                    log::info!("Maybe server was closed");
                }
                Err(ref err) => {
                    if err.is::<std::io::Error>() {
                        let error = err.downcast_ref::<std::io::Error>().unwrap();
                        log::info!("Socket: io error: {}", error);
                    } else if err.is::<serde_json::Error>() {
                        log::info!(
                            "Json: serde_json error: {}",
                            err.downcast_ref::<serde_json::Error>().unwrap()
                        );
                    } else if err.is::<quinn::ReadError>() {
                        log::info!(
                            "Quic: readError: {}",
                            err.downcast_ref::<quinn::ReadError>().unwrap()
                        );
                    }
                }
            }

            self.reconnect_to_server().await?;
        }
        // anyhow::Ok(())
    }

    async fn reconnect_to_server(&mut self) -> anyhow::Result<()> {
        let mut retry_times = self.retry_time;
        if retry_times <= 0 {
            log::info!("Reconnecting to server was disable");
            anyhow::bail!("Reconnecting to server was disable");
        }
        log::info!("Try to connect again");
        loop {
            match self
                .factory
                .connect(&self.connector, self.addr.socketaddr())
                .await
            {
                Ok(socket) => {
                    self.socket = socket;
                    log::info!("Reconnect successfully");
                    break Ok(());
                }
                Err(err) => {
                    retry_times -= 1;
                    log::info!("Left times: {}", retry_times);
                    if retry_times <= 0 {
                        log::info!("Failed to reconnect");
                        return Err(anyhow::Error::from(err));
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    async fn send_heartbeat(&mut self) -> anyhow::Result<()> {
        let msg = JsonMessage::Heartbeat;
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        Ok(())
    }

    fn recv_msg(&mut self, msg: Vec<u8>) -> anyhow::Result<()> {
        let msg = serde_json::from_slice::<server::JsonMessage>(&msg)?;
        match msg {
            server::JsonMessage::NewChannel { port, number } => {
                log::info!("Ready to accept channel: {:?} {}", port, number);
                self.accept_channel(port, number);
            }
            _ => {
                todo!()
            }
        }
        Ok(())
    }

    fn accept_channel(&self, port: Port, number: u128) {
        let id = self.id.unwrap();
        let local_addr = match self.links.get(&port) {
            Some(p) => *p,
            None => return,
        };

        let addr = self.addr.clone();
        let factory = self.factory.clone();
        let connector = self.connector.clone();
        tokio::spawn(async move {
            if let BasicProtocol::Tcp = port.protocol {
                log::info!("Start to connect to local addr: {addr:?}");
                let mut local_socket = TcpStream::connect(local_addr).await?;

                let mut socket = factory.connect(&connector, addr.socketaddr()).await?;
                let msg = JsonMessage::AcceptChannel {
                    from: id,
                    port,
                    number,
                };
                let msg = serde_json::to_vec(&msg)?;
                socket.write(&msg).await?;

                log::info!("Start to copy tcp bidirectional local_addr: {}", local_addr);
                let result = tokio::io::copy_bidirectional(&mut socket, &mut local_socket).await;
                result?;
            } else {
                let udp_socket = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
                udp_socket.connect(local_addr).await?;

                let mut socket = factory.connect(&connector, addr.socketaddr()).await?;
                let msg = JsonMessage::AcceptChannel {
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
    T: AsyncRead + AsyncWrite + Unpin,
{
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    let mut stream_buf = BytesMut::new();
    let mut socket_buf = Vec::with_capacity(4096);
    loop {
        tokio::select! {
            _ = stream.read_buf(&mut stream_buf) => {
                socket.send(&std::mem::take(&mut stream_buf)[..]).await?;
            },
            result = socket.recv_buf(&mut socket_buf) => {
                let size = result?;
                stream.write_all(&socket_buf[..size]).await?;
                socket_buf.clear();
            }
        }
    }
}
