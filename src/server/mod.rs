mod controller;
mod monitor;

use crate::protocol::udp::UdpClient;
use crate::protocol::Factory;
use crate::protocol::{
    BasicProtocol, Port, SimpleRead, SimpleStream, Address
};
use controller::TcpSocketInfo;
use controller::UdpRecverInfo;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;
use super::client;
use crate::utils::ARwLock;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    #[serde(rename="accept_client")]
    AcceptClient {
        id: u128,
    },

    #[serde(rename="new_channel")]
    NewChannel {
        port: Port,
        number: u128,
    },

    #[serde(rename="accept_config")]
    AcceptConfig(Accepted),

    #[serde(rename="heartbeat")]
    Heartbeat,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Accepted {

    #[serde(rename="all")]
    All,

    #[serde(rename="part")]
    Part(Vec<Port>),
}

pub struct Server<T>
where
    T: Factory,
{
    addr: Address,
    tcp_sockets: ARwLock<HashMap<u128, Vec<(TcpSocketInfo, oneshot::Sender<()>)>>>,
    udp_recvers: ARwLock<HashMap<u128, Vec<(UdpRecverInfo, oneshot::Sender<()>)>>>,
    using_ports: ARwLock<HashSet<Port>>,
    factory: Arc<T>,
}

impl<T> Clone for Server<T>
where
    T: Factory,
{
    fn clone(&self) -> Self {
        Self {
            addr: self.addr.clone(),
            tcp_sockets: Arc::clone(&self.tcp_sockets),
            using_ports: Arc::clone(&self.using_ports),
            udp_recvers: Arc::clone(&self.udp_recvers),
            factory: Arc::clone(&self.factory),
        }
    }
}

impl<T> Server<T>
where
    T: Factory,
    T::Socket: SimpleStream,
{
    pub fn new(addr: Address, factory: T) -> Self {
        Self {
            addr,
            tcp_sockets: Default::default(),
            using_ports: Default::default(),
            udp_recvers: Default::default(),
            factory: Arc::new(factory),
        }
    }
    // async fn bind(addr: SocketAddr) -> anyhow::Result<Self> {
    //     Ok(Self { listener: T::bind(addr).await? })
    // }

    pub fn run(self) {
        log::info!("Server start running");
        tokio::spawn(async move {
            let listener = self.factory.bind(self.addr.socketaddr()).await?;
            loop {
                match self.factory.accept(&listener).await {
                    Ok((socket, _)) => {
                        log::info!("New connection in coming");
                        self.clone().on_new_connection(socket);
                    },
                    Err(err) => return Result::<(), _>::Err(anyhow::Error::from(err)),
                }
                // let (socket, _) = self.factory.accept(&listener).await?;
                // log::info!("new connection in coming");
                // self.clone().on_new_connection(socket);
            }
        });
    }

    async fn add_waiting_udp_recver(&self, id: u128, info: UdpRecverInfo, timeout: Option<u64>) {
        let mut write_lock = self.udp_recvers.write().await;
        if !write_lock.contains_key(&id) {
            write_lock.insert(id, Default::default());
        }
        let port = info.port;
        let number = info.number;

        let (sender, recver) = oneshot::channel();
        write_lock.get_mut(&id).unwrap().push((info, sender));

        if let Some(timeout) = timeout {
            let cloned = self.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(timeout)) => {
                        cloned.take_waiting_udp_recver(id, port, number).await;
                    }
                    _ = recver => {
                        
                    }
                }
            });
        }
    }

    async fn take_waiting_udp_recver(
        &self,
        id: u128,
        port: u16,
        number: u128,
    ) -> Option<UdpRecverInfo> {
        let mut write_lock = self.udp_recvers.write().await;
        let infos = write_lock.get_mut(&id)?;
        let index = infos.iter().position(|v| v.0.port == port && v.0.number == number)?;
        Some(infos.remove(index).0)
    }

    async fn add_waiting_tcp_socket(&self, id: u128, info: TcpSocketInfo, timeout: Option<u64>) {
        let mut write_lock = self.tcp_sockets.write().await;
        if !write_lock.contains_key(&id) {
            write_lock.insert(id, Vec::new());
        }
        let infos = write_lock.get_mut(&id).unwrap();

        let port = info.port;
        let number = info.number;
        let (sender, recver) = oneshot::channel();
        infos.push((info, sender));
        let cloned = self.clone();
        if let Some(timeout) = timeout {
            tokio::spawn(async move {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(timeout)) => {
                        cloned.take_waiting_tcp_socket(id, port, number).await;
                    }
                    _ = recver => {
                        
                    }
                }
            });
        }
    }

    async fn take_waiting_tcp_socket(
        &self,
        id: u128,
        port: u16,
        number: u128,
    ) -> Option<TcpSocketInfo> {
        let mut write_lock = self.tcp_sockets.write().await;
        let infos = write_lock.get_mut(&id)?;
        let index = infos.iter().position(|v| v.0.port == port && v.0.number == number)?;
        Some(infos.remove(index).0)
    }

    fn on_new_connection(self, mut socket: T::Socket) {
        tokio::spawn(async move {
            let json = SimpleRead::read(&mut socket).await?;
            let msg = serde_json::from_slice::<client::Message>(&json)?;
            match msg {
                client::Message::NewClient => {
                    log::info!("New client connected");
                    let controller = controller::Controller::new(&self, socket);
                    controller.run();
                }
                client::Message::AcceptChannel { from, port, number } => {
                    log::info!("Channel({}) is accepted by client(id:{})", port, from);
                    if let BasicProtocol::Tcp = port.protocol {
                        self.accept_tcp_channel(from, port.port, number, socket);
                    } else {
                        self.accept_udp_channel(from, port.port, number, socket);
                    }
                }
                _ => {
                    log::warn!("Incorret client msg: {:?}", msg);
                    return Err(anyhow::anyhow!("Incorret msg {:?}", msg));
                }
            };
            anyhow::Ok(())
        });
    }

    fn accept_udp_channel(self, from: u128, port: u16, number: u128, mut socket: T::Socket) {
        tokio::spawn(async move {
            let info = self.take_waiting_udp_recver(from, port, number).await;
            let mut udp_socket = match info {
                Some(info) => info.socket,
                None => return,
            };
            let _ = copy(&mut socket, &mut udp_socket).await;
        });
    }

    fn accept_tcp_channel(self, from: u128, port: u16, number: u128, mut socket: T::Socket) {
        tokio::spawn(async move {
            let info = self.take_waiting_tcp_socket(from, port, number).await;
            let mut remote_socket = match info {
                Some(info) => info.socket,
                None => return,
            };
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut remote_socket).await;
        });
    }
}

async fn copy<T>(stream: &mut T, udp_recver: &mut UdpClient) -> anyhow::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + 'static,
{
    use tokio::time::timeout;
    loop {
        let mut buf = vec![];
        tokio::select! {
            result = stream.read_buf(&mut buf) => {
                let size = result?;
                udp_recver.write(&buf[..size]).await?;
            }
            result = timeout(Duration::from_secs(60 * 10), udp_recver.read()) => {
                let data = result??;
                stream.write_all(&data).await?;
            }
        }
    }
}
