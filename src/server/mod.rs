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
    net::SocketAddr,
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
    AcceptClient {
        id: i64,
    },

    NewChannel {
        port: Port,
        number: i64,
    },
    AcceptConfig(Accepted),

    Heartbeat,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Accepted {
    All,
    Part(Vec<Port>),
}

pub struct Server<T>
where
    T: Factory,
{
    addr: Address,
    tcp_sockets: ARwLock<HashMap<i64, Vec<(TcpSocketInfo, oneshot::Sender<()>)>>>,
    udp_recvers: ARwLock<HashMap<i64, Vec<(UdpRecverInfo, oneshot::Sender<()>)>>>,
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
        tokio::spawn(async move {
            let listener = self.factory.bind(self.addr.socketaddr()).await?;
            loop {
                match self.factory.accept(&listener).await {
                    Ok((socket, _)) => {
                        log::info!("new connection in coming");
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

    async fn add_waiting_udp_recver(&self, id: i64, info: UdpRecverInfo, timeout: Option<u64>) {
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
        id: i64,
        port: u16,
        number: i64,
    ) -> Option<UdpRecverInfo> {
        let mut write_lock = self.udp_recvers.write().await;
        let infos = write_lock.get_mut(&id)?;
        let index = infos.iter().position(|v| v.0.port == port && v.0.number == number)?;
        Some(infos.remove(index).0)
    }

    async fn add_waiting_tcp_socket(&self, id: i64, info: TcpSocketInfo, timeout: Option<u64>) {
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
        id: i64,
        port: u16,
        number: i64,
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
                    log::info!("client connected");
                    let controller = controller::Controller::new(&self, socket);
                    controller.run();
                }
                client::Message::AcceptChannel { from, port, number } => {
                    log::info!("client accepted channel: {:?}", port);
                    if let BasicProtocol::Tcp = port.protocol {
                        self.accept_tcp_channel(from, port.port, number, socket);
                    } else {
                        self.accept_udp_channel(from, port.port, number, socket);
                    }
                }
                _ => {
                    log::warn!("incorret client msg: {:?}", msg);
                    return Err(anyhow::anyhow!("incorret msg {:?}", msg));
                }
            };
            anyhow::Ok(())
        });
    }

    fn accept_udp_channel(self, from: i64, port: u16, number: i64, mut socket: T::Socket) {
        tokio::spawn(async move {
            let info = self.take_waiting_udp_recver(from, port, number).await;
            let mut udp_socket = match info {
                Some(info) => info.socket,
                None => return,
            };
            let _ = copy(&mut socket, &mut udp_socket).await;
        });
    }

    fn accept_tcp_channel(self, from: i64, port: u16, number: i64, mut socket: T::Socket) {
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

/*
use crate::protocol::{BasicProtocol, Port, self, Safe};
use crate::protocol::{SimpleRead, ClientToServer, Accepted};
use crate::{
    protocol::{ServerToClient, Protocol, SimpleWrite},
    utils::chat,
};
use fast_async_mutex::rwlock::RwLock;
use once_cell::sync::Lazy;
use tokio::io::{AsyncRead, AsyncWrite};
use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
*/

// type Leader = chat::Leader<ControllerToMonitor, MonitorToController, Port>;
// type Employee = chat::Employee<ControllerToMonitor, MonitorToController, Port>;

// pub struct Machine<T>
// where
//     T: Protocol,
// {
//     listener: T,
//     leader: chat::Leader<MachineToController<T::Socket>, ControllerToMachine>,
//     identify_leaders: chat::Leader<(), IdentifyToMachine<T>>,
// }

// impl<T: Protocol> Machine<T> {
//     fn on_identified(result: IdentifyResult<T::Socket>) {

//     }
// }

// pub struct AsyncFunc<T: FnOnce() + Safe> {
//     func: T
// }

// impl<T: FnOnce() + Safe> AsyncFunc<T>  {
//     fn new(func: T) -> Self {
//         Self { func }
//     }
//     fn run(mut self) {
//         tokio::spawn(async move {
//             (self.func)();
//         });
//     }
// }

// pub enum IdentifyToMachine<T> {
//     Fail(T),
//     Channel {
//         from: i64,
//         port: Port,
//         number: i64
//     },
//     Client(T)
// }

// pub enum IdentifyResult<T> {
//     Fail(T),
//     Channel {
//         from: i64,
//         port: Port,
//         number: i64
//     },
//     Client(T)
// }

// pub struct Identify<T: SimpleRead + Send + Sync + 'static> {
//     employee: chat::Employee<(), IdentifyToMachine<T>>,
//     socket: T
// }

// impl<T: SimpleRead + Send + Sync + 'static> Identify<T> {
//     pub fn new(socket: T, employee: chat::Employee<(), IdentifyToMachine<T>>) -> Self {
//         Self {
//             socket,
//             employee
//         }
//     }
//     pub fn run(mut self) {
//         tokio::spawn(async move {
//             let Ok(msg) = SimpleRead::read(&mut self.socket).await else {
//                 self.employee.report(IdentifyToMachine::Fail(self.socket)).unwrap();
//                 return;
//             };
//             match serde_json::from_slice::<ClientToServer>(&msg) {
//                 Ok(ClientToServer::NewClient) => {
//                     // employee.become_regular().await;
//                     self.employee.report(IdentifyToMachine::Client(self.socket)).unwrap();

//                 },
//                 Ok(ClientToServer::AcceptChannel { from, port, number: random_number }) => {
//                     self.employee.report(IdentifyToMachine::Channel{ from, port, number: random_number}).unwrap()
//                     // self.leader.send_to(&from,);
//                     // self.leader.send_to(&from, MachineToController::AcceptChannel { socket, port, number: random_number } );
//                 },
//                 _ => {
//                     self.employee.report(IdentifyToMachine::Fail(self.socket)).unwrap();
//                     log::error!("incorret msg: {:?}", msg);
//                 }
//             };
//         });
//     }
// }

// pub struct Manager {
//     using_links: RwLock<HashMap<Port, i64>>,
// }

// impl Default for Manager {
//     fn default() -> Self {
//         Manager {
//             using_links: RwLock::new(Default::default()),
//         }
//     }
// }

// impl Manager {
//     fn instance() -> &'static Manager {
//         static SELF: Lazy<Manager> = Lazy::new(|| Default::default());
//         &SELF
//     }
//     async fn push_ports<'a>(&self, links: &[Port], id: i64, force: bool) -> Option<(Vec<Port>, Vec<Port>)> {
//         let mut using_links = self.using_links.write().await;
//         if force {
//             for i in links {
//                 using_links.insert(*i, id);
//             }
//             None
//         } else {
//             let mut pushed = Vec::new();
//             let mut conflict = Vec::new();
//             for i in links {
//                 if using_links.contains_key(i) {
//                     conflict.push(*i);
//                 } else {
//                     using_links.insert(*i, id);
//                     pushed.push(*i);
//                 }
//             }
//             if conflict.is_empty() {
//                 None
//             } else {
//                 Some((pushed, conflict))
//             }
//         }
//     }

//     async fn remove_ports(&self, id: i64) {
//         let mut using_ports = self.using_links.write().await;
//         using_ports.retain(|_, v|{
//             *v != id
//         });
//     }

// }

// impl<T> Machine<T>
// where
//     T: Protocol,
// {
//     pub async fn bind(addr: SocketAddr) -> anyhow::Result<Self> {
//         anyhow::Ok(Machine {
//             listener: T::bind(addr).await?,
//             leader: Default::default(),
//             identify_leaders: Default::default(),
//         })
//     }
// }
// impl<T> Machine<T>
// where
//     T: Protocol,
//     T::Socket: SimpleRead + SimpleWrite + 'static,
// {
//     pub fn run(mut self) {
//         tokio::spawn(async move {
//             loop {

//                 tokio::select! {
//                     result = self.listener.accept() => {
//                         let (socket, _) = result?;
//                         self.on_new_connection(socket);
//                     },
//                     result = self.leader.receive() => {
//                         self.handle_controller_message(result);
//                     }
//                 };
//             }
//             anyhow::Ok(())
//         });
//     }

//     fn on_new_connection(&mut self, mut socket: T::Socket) {
//         let id = time::OffsetDateTime::now_utc().unix_timestamp();
//         let employee = self.leader.hire(&id);
//         tokio::spawn(async move {
//             let Ok(msg) = SimpleRead::read(&mut socket).await else {
//                 employee.report(ControllerToMachine::AuthenticateClientFailed)?;
//                 return Err(anyhow::anyhow!("read  error"));
//             };
//             match serde_json::from_slice::<ClientToServer>(&msg) {
//                 Ok(ClientToServer::NewClient) => {
//                     // employee.become_regular().await;
//                     let controller: Controller<T> = Controller::new(employee, socket);
//                     controller.run();
//                 },
//                 Ok(ClientToServer::AcceptChannel { from, port, number: random_number }) => {
//                     // self.leader.send_to(&from,);
//                     // self.leader.send_to(&from, MachineToController::AcceptChannel { socket, port, number: random_number } );
//                 },
//                 _ => {
//                     employee.report(ControllerToMachine::AuthenticateClientFailed)?;
//                     log::error!("incorret msg: {:?}", msg);
//                     return Err(anyhow::anyhow!("incorret msg: {:?}", msg));
//                 }
//             };
//             anyhow::Ok(())
//         });

//     }

//     fn handle_controller_message(&mut self, (id, msg): (i64, ControllerToMachine)) {
//         match msg {
//             ControllerToMachine::Disconnect => {
//                 self.leader.fired(&id);
//             },
//             ControllerToMachine::AuthenticateClientFailed => {
//                 self.leader.fired(&id);
//             },
//         }
//     }
// }

// pub struct Controller<T>
// where
//     T: Protocol
// {
//     // id: i64,// new
//     employee_of_machine: chat::Employee<MachineToController<T::Socket>, ControllerToMachine>, // new
//     leader_of_monitor: Leader,
//     socket: T::Socket,
//     waiting_tcp_stream: HashMap<u16, HashMap<i64, TcpStream>>,
//     udp_cache: HashMap<u16, Vec<u8>>,
//     links: Vec<Port>
// }

// impl<T: Protocol> Controller<T>  {
//     fn new(em: chat::Employee<MachineToController<T::Socket>, ControllerToMachine>, socket: T::Socket) -> Self {
//         Self {
//             employee_of_machine: em,
//             leader_of_monitor: Leader::new(),
//             socket,
//             waiting_tcp_stream: Default::default(),
//             udp_cache: Default::default(),
//             links: Default::default()
//         }
//     }
// }

// impl<T> Controller<T>
// where
//     T: Protocol,
//     T::Socket: SimpleWrite + SimpleRead + Send + Sync
// {

//     async fn accept_client(&mut self) -> anyhow::Result<()> {
//         let id = *self.employee_of_machine.id();
//         let msg = serde_json::to_vec(&ServerToClient::AcceptClient { id })?;
//         self.socket.write(&msg).await?;
//         anyhow::Ok(())
//     }

//     /*
//     async fn recv_config(&mut self) -> anyhow::Result<Vec<Port>> {
//         let msg = self.socket.read().await?;
//         let mut msg = serde_json::from_slice::<ClientToServer>(&msg)?;
//         let ClientToServer::PushConfig(ports) = msg else {
//             log::error!("incorret msg :{:?}", msg);
//             return Err(anyhow::anyhow!("incorret msg :{:?}", msg));
//         };
//         anyhow::Ok(ports)
//     }
//     async fn send_accept_config(&mut self, link: Option<(Vec<&Link>, Vec<&Link>)>) -> anyhow::Result<()> {
//         let msg = match link {
//             Some((first, second)) => {
//                 let first = first.into_iter().map(|v| *v).collect::<Vec<Link>>();
//                 ServerToClient::AcceptConfig(Accepted::Part(first))
//             },
//             None => ServerToClient::AcceptConfig(Accepted::All),
//         };
//         self.socket.write(&serde_json::to_vec(&msg)?).await?;
//         anyhow::Ok(())
//     }
//     */
//     async fn start_monitors(&mut self) -> anyhow::Result<()> {
//         for i in &self.links {
//             match i.protocol {
//                 BasicProtocol::Tcp => {
//                     let employee = self.leader_of_monitor.hire(i);
//                     // employee.become_regular().await;
//                     let Ok(listener) = TcpMonitor::bind(format!("0.0.0.0:{}", i.port).parse().unwrap(), employee).await else {
//                         continue;
//                     };
//                     listener.run();
//                 }
//                 BasicProtocol::Udp => {
//                     let employee = self.leader_of_monitor.hire(i);
//                     // employee.become_regular().await;
//                     let Ok(listener) = UdpMonitor::bind(format!("0.0.0.0:{}", i.port).parse().unwrap(), employee).await else {
//                         continue;
//                     };
//                     listener.run();
//                 }
//             }
//         }
//         Ok(())
//     }

//     async fn accept_config(&mut self) -> anyhow::Result<()> {

//         let msg = self.socket.read().await?;
//         let msg = serde_json::from_slice::<ClientToServer>(&msg)?;
//         let ClientToServer::PushConfig(ports) = msg else {
//             log::error!("incorret msg :{:?}", msg);
//             return Err(anyhow::anyhow!("incorret msg :{:?}", msg));
//         };

//         let msg = match Manager::instance().push_ports(&ports, *self.employee_of_machine.id(), false).await {
//             Some((accepted, _)) => {
//                 self.links = accepted.clone();
//                 Accepted::Part(accepted)
//             },
//             None => {
//                 self.links = ports;
//                 Accepted::All
//             },
//         };
//         self.socket.write(&serde_json::to_vec(&msg)?).await?;
//         Ok(())
//     }

//     async fn authenticate_client(&mut self) -> anyhow::Result<()> {

//         self.accept_client().await?;

//         self.accept_config().await?;

//         self.start_monitors().await?;

//         Ok(())
//     }

//     async fn store_tcp_stream(&mut self, port: u16, stream: TcpStream) {
//         let number = time::OffsetDateTime::now_utc().unix_timestamp();
//         match self.waiting_tcp_stream.get_mut(&port) {
//             Some(map) => {
//                 map.insert(number, stream);
//             },
//             None => {
//                 let mut map = HashMap::new();
//                 map.insert(number, stream);
//                 self.waiting_tcp_stream.insert(port, map);
//             },
//         };
//     }

//     fn run_tcp_channel(&mut self, port: u16, number: i64, mut stream: T::Socket) -> anyhow::Result<()> {
//         let Some(map) = self.waiting_tcp_stream.get_mut(&port) else {
//             // log::warn!("tcp is invalid from client:{}, port:{}", client, server_port);
//             return Err(anyhow::anyhow!(""));
//         };
//         let socket = map.remove(&number);
//         let Some(mut socket) = socket else {
//             // log::warn!("tcp is invalid from client:{}, port:{}", client, server_port);
//             return Err(anyhow::anyhow!(""));
//         };
//         tokio::spawn(async move {
//             protocol::cancelable_copy(&mut stream, &mut socket, None).await
//             // tokio::io::copy_bidirectional(&mut stream, &mut socket).await
//         });
//         Ok(())
//     }

//     fn run(mut self) {
//         tokio::spawn(async move {
//             if self.authenticate_client().await.is_err() {
//                 let id = *self.employee_of_machine.id();
//                 Manager::instance().remove_ports(id).await;
//                 self.employee_of_machine.report(ControllerToMachine::AuthenticateClientFailed)?;
//                 return Err(anyhow::anyhow!("authenticate client failed"));
//             }

//             loop {
//                 tokio::select! {
//                     msg = self.leader_of_monitor.receive() => {
//                         // if msg.is_none() {
//                         //     log::warn!("Maybe the client didn't request to listen on any port");
//                         //     log::warn!("Actively close the client connection");
//                         //     break;
//                         // }
//                         match msg.1 {
//                             MonitorToController::NewTcpConnection { port, stream } => {
//                                 self.store_tcp_stream(port, stream).await;
//                             },
//                             MonitorToController::NewUdpDatagram { port, mut data } => {
//                                 match self.udp_cache.get_mut(&port) {
//                                     Some(buf) => {
//                                         buf.append(&mut data);
//                                     },
//                                     None => {
//                                         self.udp_cache.insert(port, data);
//                                     }
//                                 }
//                             },
//                         }
//                     },
//                     result = self.socket.read() => {
//                         if result.is_err() {
//                             log::info!("read error: {:?}. This is considered an active disconnect by the client.", result);
//                             self.employee_of_machine.report(ControllerToMachine::Disconnect)?;
//                             break;
//                         }
//                     },
//                     result = self.employee_of_machine.wait() => {
//                         let msg = result.unwrap();
//                         match msg {
//                             MachineToController::AcceptChannel {
//                                 socket,
//                                 port,
//                                 number,
//                             } => {
//                                 match port.protocol {
//                                     BasicProtocol::Tcp => if self.run_tcp_channel(port.port, number, socket).is_err() {
//                                         continue;
//                                     },
//                                     BasicProtocol::Udp => todo!(),
//                                 };

//                             },
//                         }
//                     }
//                 }
//             }
//             anyhow::Ok(())
//         });
//     }
// }

// pub enum MachineToController<T>
// where
//     T: AsyncRead + AsyncWrite + Send + Sync + 'static,
// {
//     AcceptChannel {
//         socket: T,
//         port: Port,
//         number: i64,
//     }
// }

// pub enum ControllerToMachine {
//     Disconnect,
//     AuthenticateClientFailed,
// }

// pub enum MonitorToController {
//     NewTcpConnection { port: u16, stream: TcpStream },
//     NewUdpDatagram { port: u16, data: Vec<u8> },
// }

// #[derive(Clone)]
// pub enum ControllerToMonitor {
//     Close,
// }

// pub struct TcpMonitor {
//     employee: Employee,
//     port: u16,
//     listener: TcpListener,
// }

// impl TcpMonitor {
//     async fn bind(addr: SocketAddr, employee: Employee) -> io::Result<Self> {
//         Ok(Self {
//             employee,
//             port: addr.port(),
//             listener: TcpListener::bind(addr).await?,
//         })
//     }

//     fn run(mut self) {
//         tokio::spawn(async move {
//             loop {
//                 tokio::select! {
//                     ret = self.listener.accept() => {
//                         let (stream, _) = ret?;
//                         self.employee.report(MonitorToController::NewTcpConnection { port: self.port, stream })?;
//                     }
//                     _ = self.employee.wait() => {
//                         break;
//                     }
//                 }
//             }
//             anyhow::Ok(())
//         });
//     }
// }

// pub struct UdpMonitor {
//     worker: chat::Employee<ControllerToMonitor, MonitorToController, Port>,
//     port: u16,
//     socket: UdpSocket,
// }

// impl UdpMonitor {
//     async fn bind(addr: SocketAddr, worker: chat::Employee<ControllerToMonitor, MonitorToController, Port>) -> io::Result<Self> {
//         Ok(Self {
//             worker,
//             port: addr.port(),
//             socket: UdpSocket::bind(addr).await?,
//         })
//     }

//     fn run(mut self) {
//         tokio::spawn(async move {
//             let mut buf = Vec::with_capacity(4096);
//             loop {
//                 tokio::select! {
//                     ret = self.socket.recv_buf(&mut buf) => {
//                         let size = ret?;
//                         self.worker.report(MonitorToController::NewUdpDatagram { port: self.port, data: buf[..size].to_vec() })?;
//                     }
//                     _ = self.worker.wait() => {
//                         break;
//                     }
//                 }
//             }
//             anyhow::Ok(())
//         });
//     }
// }

// pub struct PortListener {
//     pub port: u16,
//     pub listener: Listener,
//     pub worker: Worker
// }

// impl PortListener {
//     async fn bind_tcp(addr: SocketAddr, worker: Worker) -> anyhow::Result<Self> {
//         anyhow::Ok(Self {
//             port: addr.port(),
//             listener: Listener::bind_tcp(addr).await?,
//             worker
//         })
//     }
//     async fn bind_udp(addr: SocketAddr, worker: Worker) -> anyhow::Result<Self> {
//         anyhow::Ok(Self {
//             port: addr.port(),
//             listener: Listener::bind_udp(addr).await?,
//             worker
//         })
//     }

//     async fn exec(&self) -> anyhow::Result<()> {
//         match self.listener {
//             Listener::Tcp(ref listner) => {
//                 let (stream, _) = listner.accept().await?;
//                 self.worker.report(ListenerMessage::NewTcpConnection { port: self.port, stream }).await?;
//             },
//             Listener::Udp(ref socket) => {
//                 let mut buf = Vec::with_capacity(4096 * 5);
//                 let size = socket.recv_buf(&mut buf).await?;
//                 self.worker.report(
//                     ListenerMessage::NewUdpDatagram {
//                         port: self.port,
//                         data: buf[..size].to_vec(),
//                     })
//                     .await?;
//             },
//         }
//         anyhow::Ok(())
//     }

// async fn wait(&mut self) -> anyhow::Result<()> {
//     match self.worker.accpet() {
//         Some(ref mut recver) =>{
//             recver.changed().await?;
//         },
//         None => Never::never().await,
//     };
//     anyhow::Ok(())
// }

// fn run(mut self) {
//     tokio::spawn(async move {
//         loop {
//             tokio::select! {
//                 ret = self.exec() => {
//                     if ret.is_err() {
//                         break;
//                     }
//                 },
//                 _ = self.worker.accpet() => {
//                     break;
//                 }
//             }
//         }
//     });
// }
// fn run(mut self) {
//     tokio::spawn(async move {
//         match self.listener {
//             Listener::Tcp(listener) => loop {
//                 tokio::select! {
//                     ret = listener.accept() => {
//                         self.sender
//                         .send(ListenerMessage::NewTcpConnection {
//                             port: self.port,
//                             stream: ret?.0,
//                         })
//                         .await?;
//                     },
//                     _ = self.recver.wait() => break,
//                 }
//                 // let (stream, _) = listener.accept().await?;
//                 // self.sender
//                 //     .send(ListenerMessage::NewTcpConnection {
//                 //         port: self.port,
//                 //         stream,
//                 //     })
//                 //     .await?;
//             }
//             Listener::Udp(socket) => loop {
//                 let mut buf = Vec::with_capacity(4096 * 5);
//                 let size = socket.recv_buf(&mut buf).await?;
//                 self.sender
//                     .send(ListenerMessage::NewUdpDatagram {
//                         port: self.port,
//                         data: buf[..size].to_vec(),
//                     })
//                     .await?;
//             }
//         };

//         anyhow::Ok(())
//     });
// }
// }

// pub enum Listener {
//     Tcp(TcpListener),
//     Udp(UdpSocket),
// }

// impl Listener {
//     async fn bind_tcp(addr: SocketAddr) -> anyhow::Result<Self> {
//         anyhow::Ok(Listener::Tcp(TcpListener::bind(addr).await?))
//     }
//     async fn bind_udp(addr: SocketAddr) -> anyhow::Result<Self> {
//         anyhow::Ok(Listener::Udp(UdpSocket::bind(addr).await?))
//     }
// }
