use std::time::Duration;
use std::{
    collections::HashSet,
    net::SocketAddr,
};

use tokio::{
    net::TcpStream,
    io::AsyncReadExt,
};
use super::{Server, monitor, Message as ServerMessage, Accepted};
use crate::protocol::{SimpleWrite, BasicProtocol};
use crate::protocol::udp::UdpClient;
use crate::{
    protocol::{Port, Factory, SimpleRead, SimpleStream},
    utils::chat,
};

use crate::client;


pub enum Message {
    NewTcp(TcpStream),
    NewUdp(UdpClient),
    Error,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct UdpAddress {
    port: u16,
    addr: SocketAddr,
}

pub struct UdpRecverInfo {
    pub port: u16,
    pub number: i64,
    pub socket: UdpClient,
}

pub struct TcpSocketInfo {
    pub number: i64,
    pub port: u16,
    pub socket: TcpStream,
}

pub struct Controller<T: Factory> {
    server: Server<T>,
    socket: T::Socket,
    id: Option<i64>,
    ports: HashSet<Port>,
    leader: chat::Leader<(), Message, Port>,
}

impl<T: Factory> Controller<T>
where
    T::Socket: SimpleStream,
{
    pub fn new(server: &Server<T>, socket: T::Socket) -> Self {
        Self {
            server: server.clone(),
            socket,
            id: None,
            ports: Default::default(),
            leader: Default::default(),
        }
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            self.accept_client().await?;
            self.accept_config().await?;
            self.start_monitors().await;
            use tokio::time::timeout;
            loop {
                tokio::select! {
                    result = timeout(Duration::from_secs(5), SimpleRead::read(&mut self.socket)) => {
                        match result {
                            Ok(Ok(msg)) if msg.len() != 0 => {
                                self.recv_msg(msg)?;
                            }
                            _ => {
                                log::info!("Client disconnect {}", self.id.unwrap());
                                self.cleanup().await;
                                break;                                
                            }
                        }
                    }
                    result = self.leader.receive() => {
                        let (port, msg) = result;
                        self.new_channel(port, msg).await?;
                    }
                }
            }
            anyhow::Ok(())
        });
    }
    
    fn recv_msg(&self, msg: Vec<u8>) -> anyhow::Result<()> {
        let msg: client::Message = serde_json::from_slice(&msg)?;
        match msg {
            client::Message::Heartbeat => {
                log::trace!("heartbeat from client");
            },
            _ => {
                log::warn!("undefined msg: {:?}", msg);
            }
        }
        Ok(())
    }

    async fn new_channel(&mut self, port: Port, msg: Message) -> anyhow::Result<()> {
        match msg {
            Message::NewTcp(socket) => {
                log::info!("New tcp channel");
                self.new_tcp_channel(port, socket).await?;
            }
            Message::NewUdp(socket) => {
                log::info!("New udp channel");
                self.new_udp_channel(
                    port,
                    socket
                )
                .await?;
            }
            Message::Error => {
                log::warn!("some port maybe is used");
                self.leader.fired(&port);
            }
        }
        anyhow::Ok(())
    }

    async fn new_udp_channel(
        &mut self,
        port: Port,
        socket: UdpClient
    ) -> anyhow::Result<()> {
        let number = time::OffsetDateTime::now_utc().unix_timestamp();
        self.server
            .add_waiting_udp_recver(
                self.id.unwrap(),
                UdpRecverInfo { port: port.port, number, socket },
                Some(60 * 10),
            )
            .await;
        let msg = ServerMessage::NewChannel {
            port,
            number,
        };
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        log::info!("Request udp channel");
        Ok(())
    }

    async fn new_tcp_channel(&mut self, port: Port, socket: TcpStream) -> anyhow::Result<()> {
        let number = time::OffsetDateTime::now_utc().unix_timestamp();
        let msg = ServerMessage::NewChannel {
            port,
            number,
        };
        let msg = serde_json::to_vec(&msg)?;
        self.server
            .add_waiting_tcp_socket(
                self.id.unwrap(),
                TcpSocketInfo {
                    number,
                    port: port.port,
                    socket,
                },
                Some(60 * 10),
            )
            .await;
        self.socket.write(&msg).await?;
        log::info!("Request client tcp channel");
        Ok(())
    }

    async fn cleanup(&self) {
        self.leader.broadcast(&()).await;
        let mut write_lock = self.server.tcp_sockets.write().await;
        write_lock.remove(&self.id.unwrap());
        let mut write_lock = self.server.using_ports.write().await;
        write_lock.retain(|port| !self.ports.contains(port));
    }

    async fn accept_client(&mut self) -> anyhow::Result<()> {
        let id = time::OffsetDateTime::now_utc().unix_timestamp();
        let msg = ServerMessage::AcceptClient { id };
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        self.id = Some(id);
        anyhow::Ok(())
    }

    async fn accept_config(&mut self) -> anyhow::Result<()> {
        let msg = SimpleRead::read(&mut self.socket).await?;
        let client::Message::PushConfig(ports) = serde_json::from_slice::<client::Message>(&msg)? else {
            return Err(anyhow::anyhow!("incorret msg"));
        };
        let mut success = Vec::new();
        let mut failed = Vec::new();
        let mut write_lock = self.server.using_ports.write().await;
        for i in ports {
            if write_lock.contains(&i) {
                failed.push(i);
            } else {
                write_lock.insert(i);
                success.push(i);
            }
        }
        let msg = if failed.is_empty() {
            ServerMessage::AcceptConfig(Accepted::All)
        } else {
            ServerMessage::AcceptConfig(Accepted::Part(success.clone()))
        };
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        self.ports = success.iter().map(|v| *v).collect::<HashSet<Port>>();
        anyhow::Ok(())
    }

    async fn start_monitors(&mut self) {
        for i in &self.ports {
            log::info!("Start listening: {:?}", i);
            if let BasicProtocol::Tcp = i.protocol {
                let tcp_monitor = monitor::Tcp::new(*i, self.leader.hire(i));
                tcp_monitor.run();
            } else {
                let udp_monitor = monitor::Udp::new(*i, self.leader.hire(i));
                udp_monitor.run();
            }
        }
    }
}
