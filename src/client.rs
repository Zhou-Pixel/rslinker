use crate::protocol::{BasicProtocol, Factory, Port, SimpleRead, SimpleWrite};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, UdpSocket},
};

use super::server::{Accepted, Message as ServerMessage};

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    NewClient,
    AcceptChannel {
        from: i64,
        // server_port: u16,
        // client_port: u16,
        // protocol: BasicProtocol,
        port: Port,
        number: i64,
    },
    PushConfig {
        ports: Vec<Port>,
        heartbeat_interval: u64
    },
    Heartbeat,
}

pub struct Client<T>
where
    T: Factory,
{
    addr: SocketAddr,
    socket: T::Socket,
    id: Option<i64>,
    links: HashMap<Port, SocketAddr>,
    // tcp_links: HashMap<u16, u16>,
    // udp_links: HashMap<u16, u16>,
    accept_conflict: bool,
    factory: Arc<T>,
    heartbeat_interval: u64,
    retry_time: u32
    // _marker: PhantomData<T>
}

impl<T: Factory> Client<T> {
    pub fn set_links(&mut self, links: HashMap<Port, SocketAddr>) {
        self.links = links;
    }

    pub fn set_accept_conflict(&mut self, accept_conflict: bool) {
        self.accept_conflict = accept_conflict;
    }

    pub fn set_heartbeat_interval(&mut self, heartbeat_interval: u64) {
        self.heartbeat_interval = heartbeat_interval;
    }

    pub fn set_retry_times(&mut self, retry_times: u32) {
        self.retry_time = retry_times;
    }


    pub async fn connect(addr: SocketAddr, factory: T) -> anyhow::Result<Self> {
        log::info!("Try to connect to server: {addr}");
        let socket = factory.connect(addr).await?;
        log::info!("Successfully connected to the server: {addr}");
        Ok(Self {
            addr,
            socket,
            id: None,
            links: Default::default(),
            // tcp_links: Default::default(),
            // udp_links: Default::default(),
            accept_conflict: false,
            factory: Arc::new(factory),
            heartbeat_interval: 1000,
            retry_time: 0
        })
    }

    async fn new_client(&mut self) -> anyhow::Result<()> {
        let msg = Message::NewClient;
        let json = serde_json::to_vec(&msg)?;
        self.socket.write(&json).await?;

        let msg = self.socket.read().await?;
        let msg = serde_json::from_slice::<ServerMessage>(&msg)?;
        if let ServerMessage::AcceptClient { id } = msg {
            self.id = Some(id);
            log::info!("The server accepted the client, id: {id}");
            Ok(())
        } else {
            log::info!("Server rejected client");
            Err(anyhow::anyhow!("incorret msg: {:?}", msg))
        }
    }

    async fn push_config(&mut self) -> anyhow::Result<()> {
        let config: Vec<Port> = self.links.iter().map(|(k, _)| *k).collect();
        let msg = Message::PushConfig {
            ports: config,
            heartbeat_interval: self.heartbeat_interval
        };
        let msg = serde_json::to_vec(&msg)?;
        log::info!("Start to push config");
        self.socket.write(&msg).await?;

        log::info!("Finish pushing config end");

        let msg = self.socket.read().await?;
        let msg = serde_json::from_slice::<ServerMessage>(&msg)?;

        let id = self.id.unwrap();
        match msg {
            ServerMessage::AcceptConfig(Accepted::All) => {
                log::info!("The server accepts all configuration, id: {id}");
                Ok(())
            }
            ServerMessage::AcceptConfig(Accepted::Part(ports)) if self.accept_conflict => {
                if ports.is_empty() && !self.links.is_empty() {
                    log::error!("No config was accepted");
                    Err(anyhow::anyhow!("no config was accepted"))
                } else {
                    log::info!("The server only accepted a partial configuration, id: {id}");
                    self.links.retain(|k, _| ports.contains(&k));

                    Ok(())
                }
            }
            _ => {
                log::info!("Push configuration failed, id: {id}");
                Err(anyhow::anyhow!("push config failed: {:?}", msg))
            }
        }
    }

    pub async fn exec(&mut self) -> anyhow::Result<()> {
        loop {
            let result = async {
                self.new_client().await?;
                self.push_config().await?;
                let mut interval = tokio::time::interval(Duration::from_millis(self.heartbeat_interval));
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
            }.await;

            log::info!("Retry to connect: {:?}", result);


            self.reconnect_to_server().await?;
        }
        // anyhow::Ok(())
    }

    async fn reconnect_to_server(&mut self) -> anyhow::Result<()> {
        let mut retry_times = self.retry_time;
        if retry_times <= 0 {
            return Err(anyhow::anyhow!("set retry_times > 0 to enable reconnect to server"));
        }
        loop {
            match self.factory.connect(self.addr).await {
                Ok(socket) => {
                    self.socket = socket;
                    log::info!("Reconnect successfully");
                    break anyhow::Ok(());
                },
                Err(err) => {
                    retry_times -= 1;
                    log::info!("Left times: {}", retry_times);
                    if retry_times <= 0 {
                        log::info!("Failed to reconnect");
                        return Err(anyhow::Error::from(err));
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                },
            }
        }
    }

    async fn send_heartbeat(&mut self) -> anyhow::Result<()> {
        let msg = Message::Heartbeat;
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        Ok(())
    }

    fn recv_msg(&mut self, msg: Vec<u8>) -> anyhow::Result<()> {
        let msg = serde_json::from_slice::<ServerMessage>(&msg)?;
        match msg {
            ServerMessage::NewChannel { port, number } => {
                self.accept_channel(port, number);
            }
            _ => {
                todo!()
            }
        }
        Ok(())
    }

    fn accept_channel(&self, port: Port, number: i64) {
        let id = self.id.unwrap();
        let local_addr = match self.links.get(&port) {
            Some(p) => *p,
            None => return,
        };

        let addr = self.addr;
        let factory = self.factory.clone();
        tokio::spawn(async move {
            if let BasicProtocol::Tcp = port.protocol {
                let mut local_socket = TcpStream::connect(local_addr).await?;

                let mut socket = factory.connect(addr).await?;
                let msg = Message::AcceptChannel {
                    from: id,
                    port,
                    number,
                };
                let msg = serde_json::to_vec(&msg)?;
                socket.write(&msg).await?;

                log::info!("copy tcp bidirectional local_addr: {}", local_addr);
                let result = tokio::io::copy_bidirectional(&mut socket, &mut local_socket).await;
                log::info!("copy tcp result: {:?}", result);
                result?;
            } else {
                let udp_socket = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
                udp_socket
                    .connect(local_addr)
                    .await?;

                let mut socket = factory.connect(addr).await?;
                let msg = Message::AcceptChannel {
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
    T: AsyncRead + AsyncWrite + Unpin
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
