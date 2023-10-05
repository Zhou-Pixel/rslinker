use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use super::{monitor, Accepted, JsonMessage, Server};
use crate::client;
use crate::protocol::udp::UdpClient;
use crate::protocol::{BasicProtocol, SimpleWrite};
use crate::{
    protocol::{Factory, Port, SimpleRead, SimpleStream},
    utils::chat,
};
use fast_async_mutex::RwLock;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use uuid::Uuid;

enum LocalSocket {
    Tcp(TcpStream),
    Udp(UdpClient),
}

impl LocalSocket {
    fn is_tcp(&self) -> bool {
        match self {
            LocalSocket::Tcp(_) => true,
            LocalSocket::Udp(_) => false,
        }
    }

    fn is_udp(&self) -> bool {
        match self {
            LocalSocket::Tcp(_) => false,
            LocalSocket::Udp(_) => true,
        }
    }

    fn unwrap_tcp(self) -> TcpStream {
        match self {
            LocalSocket::Tcp(socket) => socket,
            LocalSocket::Udp(_) => panic!("Not a Tcp socket"),
        }
    }

    fn unwrap_udp(self) -> UdpClient {
        match self {
            LocalSocket::Tcp(_) => panic!("Not a Udp socket"),
            LocalSocket::Udp(socket) => socket,
        }
    }
}

struct PendingSocket {
    socket: LocalSocket,
    port: u16,
}

pub enum ChannelMessage {
    Finished(anyhow::Result<()>),
}

type ServerEmployee<T> = chat::Employee<super::ChannelMessage<T>, ChannelMessage, u128>;
type MonitorLeader = chat::Leader<(), monitor::ChannelMessage, Port>;
type PendingSockets = Arc<RwLock<HashMap<u128, (PendingSocket, oneshot::Sender<()>)>>>;

pub struct Controller<T: Factory> {
    server: Server<T>,
    socket: T::Socket,
    id: u128,
    heartbeat_interval: Option<u64>,
    ports: HashSet<Port>,
    pending_sockets: PendingSockets,
    server_employee: ServerEmployee<T::Socket>,
    monitor_leader: MonitorLeader,
}

impl<T: Factory> Controller<T>
where
    T::Socket: SimpleStream,
{
    pub async fn from_server(server: &Server<T>, socket: T::Socket) -> Self {
        let id = Uuid::new_v4().as_u128();
        Self {
            server: server.clone(),
            socket,
            id,
            heartbeat_interval: None,
            pending_sockets: Default::default(),
            server_employee: server.controller_leader.write().await.hire(&id),
            ports: Default::default(),
            monitor_leader: Default::default(),
        }
    }

    async fn prepare(&mut self) -> anyhow::Result<()> {
        self.accept_client().await?;
        self.accept_config().await?;
        self.start_monitors();
        Ok(())
    }

    async fn main_loop(&mut self) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                result = {
                    let socket = &mut self.socket;
                    async {
                        let result = if let Some(interval) = self.heartbeat_interval {
                            tokio::time::timeout(Duration::from_millis(interval * 10), SimpleRead::read(socket)).await??
                        } else {
                            SimpleRead::read(socket).await?
                        };
                        anyhow::Ok(result)
                    }
                } => {
                    let msg = result?;
                    if msg.len() > 0 {
                        self.recv_msg(msg)?;
                    } else {
                        anyhow::bail!("Client disconnect nomarlly");
                    }
                },

                result = self.monitor_leader.receive() => {
                    let (port, msg) = result;
                    self.new_channel(port, msg).await?;
                }

                result = self.server_employee.wait() => {
                    let msg = result.ok_or(anyhow::anyhow!("Server has been drop"))?;
                    self.server_msg(msg);
                }
            }
        }
    }

    async fn exec(&mut self) -> anyhow::Result<()> {
        let result: anyhow::Result<()> = async {
            self.prepare().await?;
            self.main_loop().await
        }
        .await;

        log::info!("Client diconnect, state: {:?}", result);
        let msg = ChannelMessage::Finished(result);
        self.server_employee.report(msg)?;
        self.cleanup().await;
        Ok(())
    }

    pub fn run(mut self) {
        tokio::spawn(async move { self.exec().await });
    }

    fn recv_msg(&self, msg: Vec<u8>) -> anyhow::Result<()> {
        let msg: client::JsonMessage = serde_json::from_slice(&msg)?;
        match msg {
            client::JsonMessage::Heartbeat => {
                log::trace!("Heartbeat from client");
            }
            _ => {
                log::warn!("Undefined msg: {:?}", msg);
            }
        }
        Ok(())
    }

    fn server_msg(&self, msg: super::ChannelMessage<T::Socket>) {
        match msg {
            super::ChannelMessage::NewSocket {
                port,
                number,
                socket,
            } => {
                if let BasicProtocol::Tcp = port.protocol {
                    self.make_tcp_channel(port.port, number, socket);
                } else {
                    self.make_udp_channel(port.port, number, socket);
                }
            }
        }
    }

    async fn add_pending_socket(
        &self,
        number: u128,
        socket: PendingSocket,
        timeout: Option<Duration>,
    ) {
        let mut write_lock = self.pending_sockets.write().await;

        let (sender, recver) = oneshot::channel();

        write_lock.insert(number, (socket, sender));

        let pending_sockets = self.pending_sockets.clone();

        if let Some(duration) = timeout {
            tokio::spawn(async move {
                tokio::select! {
                    _ = recver => {

                    },
                    _ = tokio::time::sleep(duration) => {
                        pending_sockets.write().await.remove(&number);
                    }
                }
            });
        }
    }

    fn make_tcp_channel(
        &self,
        port: u16,
        number: u128,
        mut socket: T::Socket,
    ) {
        let pending_sockets = self.pending_sockets.clone();
        tokio::spawn(async move {
            let mut write_lock = pending_sockets.write().await;

            let (mut tcp_pending_socket, sender) = match write_lock.get(&number) {
                Some((socket, _)) if socket.port == port && socket.socket.is_tcp() => {
                    let (socket, sender) = write_lock.remove(&number).unwrap();
                    (socket.socket.unwrap_tcp(), sender)
                }
                _ => {
                    log::error!("Can't find the specified socket");
                    anyhow::bail!("Can't find the specified socket");
                }
            };

            drop(write_lock);
            let _ = sender.send(());

            log::info!("Tcp: starting copying");

            tokio::io::copy_bidirectional(&mut socket, &mut tcp_pending_socket).await?;
            anyhow::Ok(())
        });

        // let Some((tcp_pending_socket, sender)) = write_lock.remove(&number) else {
        //     log::error!("Can't find the specified socket");
        //     anyhow::bail!("Can't find the specified socket");
        // };

        // let pending_port = tcp_pending_socket.port;

        // if pending_port != port {
        //     write_lock.insert(number, (tcp_pending_socket, sender));
        //     log::error!("Two port numbers are not equal {}<=>{}", pending_port, port);
        //     anyhow::bail!("Two port numbers are not equal {}<=>{}", pending_port, port);
        // }

    }

    fn make_udp_channel(
        &self,
        port: u16,
        number: u128,
        mut socket: T::Socket,
    ) {
        let pending_sockets = self.pending_sockets.clone();
        tokio::spawn(async move {
            let mut write_lock = pending_sockets.write().await;

            let (mut udp_pending_socket, sender) = match write_lock.get(&number) {
                Some((socket, _)) if socket.port == port && socket.socket.is_udp() => {
                    let (socket, sender) = write_lock.remove(&number).unwrap();
                    (socket.socket.unwrap_udp(), sender)
                }
                _ => {
                    log::error!("Can't find the specified socket");
                    anyhow::bail!("Can't find the specified socket");
                }
            };

            drop(write_lock);
            let _ = sender.send(());

            super::copy(&mut socket, &mut udp_pending_socket).await
        });
    }

    async fn new_channel(
        &mut self,
        port: Port,
        msg: monitor::ChannelMessage,
    ) -> anyhow::Result<()> {
        let socket = match msg {
            monitor::ChannelMessage::NewTcp(socket) => {
                log::info!("New tcp channel");
                LocalSocket::Tcp(socket)
            }
            monitor::ChannelMessage::NewUdp(socket) => {
                log::info!("New udp channel");
                LocalSocket::Udp(socket)
            }
            monitor::ChannelMessage::Error => {
                log::warn!("Port({port}) may already be occupied");
                self.monitor_leader.fired(&port);
                return Ok(());
            }
        };
        let socket = PendingSocket {
            socket,
            port: port.port,
        };

        let number = Uuid::new_v4().as_u128();
        self.add_pending_socket(number, socket, Some(Duration::from_secs(300)))
            .await;
        self.notify_client(port, number).await?;
        anyhow::Ok(())
    }

    async fn notify_client(&mut self, port: Port, number: u128) -> anyhow::Result<()> {
        let msg = JsonMessage::NewChannel { port, number };
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        Ok(())
    }

    // async fn new_udp_channel(&mut self, port: Port, socket: UdpClient) -> anyhow::Result<()> {
    //     let number = Uuid::new_v4().as_u128();
    //     self.server
    //         .add_waiting_udp_recver(
    //             self.id,
    //             UdpRecverInfo {
    //                 port: port.port,
    //                 number,
    //                 socket,
    //             },
    //             Some(60 * 10),
    //         )
    //         .await;

    //     let msg = JsonMessage::NewChannel { port, number };
    //     let msg = serde_json::to_vec(&msg)?;
    //     self.socket.write(&msg).await?;
    //     log::info!("Request udp channel");
    //     Ok(())
    // }

    // async fn new_tcp_channel(&mut self, port: Port, socket: TcpStream) -> anyhow::Result<()> {
    //     let number = Uuid::new_v4().as_u128();
    //     let msg = JsonMessage::NewChannel { port, number };
    //     let msg = serde_json::to_vec(&msg)?;
    //     self.server
    //         .add_waiting_tcp_socket(
    //             self.id,
    //             TcpSocketInfo {
    //                 number,
    //                 port: port.port,
    //                 socket,
    //             },
    //             Some(60 * 10),
    //         )
    //         .await;
    //     self.socket.write(&msg).await?;
    //     log::info!("Request client tcp channel");
    //     Ok(())
    // }

    async fn cleanup(&self) {
        self.monitor_leader.broadcast(&()).await;

        let mut write_lock = self.server.using_ports.write().await;
        write_lock.retain(|port| !self.ports.contains(port));
    }

    async fn accept_client(&mut self) -> anyhow::Result<()> {
        let msg = JsonMessage::AcceptClient { id: self.id };
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        anyhow::Ok(())
    }

    async fn accept_config(&mut self) -> anyhow::Result<()> {
        let msg = SimpleRead::read(&mut self.socket).await?;
        let client::JsonMessage::PushConfig {
            ports,
            heartbeat_interval,
        } = serde_json::from_slice::<client::JsonMessage>(&msg)?
        else {
            anyhow::bail!("Incorrent msg");
        };

        self.heartbeat_interval = heartbeat_interval;

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
            JsonMessage::AcceptConfig(Accepted::All)
        } else {
            JsonMessage::AcceptConfig(Accepted::Part(success.clone()))
        };
        log::info!("Accept config: {:?}", msg);
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write(&msg).await?;
        self.ports = success.iter().map(|v| *v).collect();
        anyhow::Ok(())
    }

    fn start_monitors(&mut self) {
        for i in &self.ports {
            log::info!("Start listening: {}", i);
            let addr: SocketAddr = (self.server.addr.ip(), i.port).into();
            if let BasicProtocol::Tcp = i.protocol {
                let tcp_monitor = monitor::Tcp::new(addr, self.monitor_leader.hire(i));
                tcp_monitor.run();
            } else {
                let udp_monitor = monitor::Udp::new(addr, self.monitor_leader.hire(i));
                udp_monitor.run();
            }
        }
    }
}
