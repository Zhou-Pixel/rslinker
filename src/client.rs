use crate::{
    llhttp::RequestParser,
    protocol::{CacheAddr, Factory, Port, SimpleRead, SimpleWrite},
};
use unicase::Ascii;
use bytes::{BytesMut, BufMut};
use fast_async_mutex::RwLock;

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

#[derive(Debug, Clone)]
pub enum TransferProtocol {
    RawTcp(CacheAddr),
    RawUdp(CacheAddr),
    Http {
        auto_override: bool,
        override_: HashMap<Ascii<String>, String>,
        addr: CacheAddr,
    },
}

#[derive(Default, Debug, Clone)]
pub struct ClientConfig {
    pub links: HashMap<Port, TransferProtocol>,
    pub accept_conflict: bool,
    pub heartbeat_interval: Option<u64>,
    pub retry_times: u32,
}

#[derive(Debug)]
pub struct Client<T>
where
    T: Factory,
{
    addr: CacheAddr,
    socket: Arc<RwLock<T::Socket>>,
    connector: Arc<T::Connector>,
    id: Option<u128>,
    factory: Arc<T>,
    config: ClientConfig,
}

impl<T> Clone for Client<T>
where
    T: Factory,
{
    fn clone(&self) -> Self {
        Self {
            addr: self.addr.clone(),
            socket: self.socket.clone(),
            connector: self.connector.clone(),
            id: self.id,
            factory: self.factory.clone(),
            config: self.config.clone(),
        }
    }
}

impl<T: Factory> Client<T> {
    pub async fn connect(
        addr: CacheAddr,
        factory: T,
        config: ClientConfig,
    ) -> anyhow::Result<Self> {
        log::info!("Try to connect to server: {addr}");
        let connector = factory.make().await?;
        let socket = factory.connect(&connector, addr.socketaddr()).await?;
        log::info!("Successfully connected to the server: {addr}");
        Ok(Self {
            addr,
            socket: Arc::new(RwLock::new(socket)),
            connector: Arc::new(connector),
            id: None,
            factory: Arc::new(factory),
            config,
        })
    }

    async fn new_client(&mut self) -> anyhow::Result<()> {
        let msg = JsonMessage::NewClient;
        let json = serde_json::to_vec(&msg)?;
        let mut socket = self.socket.write().await;
        socket.write(&json).await?;

        let msg = socket.read().await?;
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
        let config: Vec<Port> = self.config.links.iter().map(|(k, _)| *k).collect();
        let msg = JsonMessage::PushConfig {
            ports: config,
            heartbeat_interval: self.config.heartbeat_interval,
        };
        let msg = serde_json::to_vec(&msg)?;
        log::info!(
            "Start to push config: {}",
            String::from_utf8(msg.to_vec()).unwrap()
        );
        let mut socket = self.socket.write().await;
        socket.write(&msg).await?;

        log::info!("Finish pushing config end");

        let msg = socket.read().await?;
        let msg = serde_json::from_slice::<server::JsonMessage>(&msg)?;

        let id = self.id.unwrap();
        match msg {
            server::JsonMessage::AcceptConfig(Accepted::All) => {
                log::info!("The server accepts all configuration, id: {id}");
                Ok(())
            }
            server::JsonMessage::AcceptConfig(Accepted::Part(ports))
                if self.config.accept_conflict =>
            {
                if ports.is_empty() && !self.config.links.is_empty() {
                    log::error!("No config was accepted");
                    Err(anyhow::anyhow!("No config was accepted"))
                } else {
                    log::info!("The server only accepted a partial configuration, id: {id}");
                    self.config.links.retain(|k, _| ports.contains(&k));

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
                result = async {
                    self.socket.write().await.read().await
                } => {
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
            let msg = self.socket.write().await.read().await?;
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
                if let Some(heartbeat_interval) = self.config.heartbeat_interval {
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
        let mut retry_times = self.config.retry_times;
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
                    self.socket = Arc::new(RwLock::new(socket));
                    log::info!("Reconnect successfully");
                    break Ok(());
                }
                Err(err) => {
                    retry_times -= 1;
                    log::info!("Left times: {}", retry_times);
                    if retry_times <= 0 {
                        log::info!("Failed to reconnect");
                        return Err(err.into());
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    async fn send_heartbeat(&mut self) -> anyhow::Result<()> {
        let msg = JsonMessage::Heartbeat;
        let msg = serde_json::to_vec(&msg)?;
        self.socket.write().await.write(&msg).await?;
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
        let Some(transfer) = self.config.links.get(&port) else { return; };
        let channel = Channel {
            id: self.id.unwrap(),
            addr: self.addr.clone(),
            factory: self.factory.clone(),
            connector: self.connector.clone(),
            port,
            number,
        };

        channel.start(transfer.clone());
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            self.exec().await?;
            anyhow::Ok(())
        });
    }
}

pub struct Channel<T>
where
    T: Factory,
{
    id: u128,
    addr: CacheAddr,
    factory: Arc<T>,
    connector: Arc<T::Connector>,
    number: u128,
    port: Port,
}

impl<T: Factory> Channel<T> {
    fn start(self, transfer: TransferProtocol) {
        log::info!("start handle channel");
        match transfer {
            TransferProtocol::RawTcp(addr) => {
                self.start_tcp_channel(addr.socketaddr());
            }
            TransferProtocol::RawUdp(addr) => self.start_udp_channel(addr.socketaddr()),
            TransferProtocol::Http {
                auto_override,
                override_,
                addr,
            } => self.start_http_channel(addr, auto_override, override_),
        }
    }

    fn start_tcp_channel(self, local_addr: SocketAddr) {
        log::info!("start tcp channel");
        tokio::spawn(async move {
            let mut local_socket = TcpStream::connect(local_addr).await?;
            let mut socket = self
                .factory
                .connect(&self.connector, self.addr.socketaddr())
                .await?;
            self.accept_channel(&mut socket).await?;

            log::info!("Start to copy tcp bidirectional local_addr: {}", local_addr);
            let result = tokio::io::copy_bidirectional(&mut socket, &mut local_socket).await;
            result?;

            anyhow::Ok(())
        });
    }

    fn start_udp_channel(self, local_addr: SocketAddr) {
        log::info!("start udp channel");
        tokio::spawn(async move {
            let udp_socket = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
            udp_socket.connect(local_addr).await?;

            let mut socket = self
                .factory
                .connect(&self.connector, self.addr.socketaddr())
                .await?;

            self.accept_channel(&mut socket).await?;

            copy(&mut socket, &udp_socket).await?;

            anyhow::Ok(())
        });
    }

    fn start_http_channel(
        self,
        local_addr: CacheAddr,
        auto_override: bool,
        override_: HashMap<Ascii<String>, String>,
    ) {
        use tokio::io::*;
        log::info!("start http channel");
        tokio::spawn(async move {
            let mut local_socket = TcpStream::connect(local_addr.socketaddr()).await?;
            let mut remote_socket = self
                .factory
                .connect(&self.connector, self.addr.socketaddr())
                .await?;
            self.accept_channel(&mut remote_socket).await?;

            let mut local_buf = BytesMut::new();
            let mut remote_buf = BytesMut::new();
            let mut override_remote_buf = BytesMut::new();
            let mut request_parser = RequestParser::new();

            let mut override_headers = HashMap::new();
            if auto_override {
                match local_addr {
                    CacheAddr::SocketAddr(ref addr) => {
                        override_headers.insert(Ascii::new("Host".to_string()), addr.to_string());
                    },
                    CacheAddr::Domain(ref domain, _) =>  {
                        override_headers.insert(Ascii::new("Host".to_string()), domain.to_string());
                    },
                }
            }

            for (key, value) in override_.iter() {
                override_headers.insert(key.to_owned(), value.to_owned());
            }

            let override_header_fn = |headers: &HashMap<Ascii<String>, String>| {

                let mut buf = String::new();

                for (key, value) in headers {
                    let header = if let Some(v) = override_headers.get(&key) {
                        format!("{}: {}\r\n", key.as_str(), v)
                    } else {
                        format!("{}: {}\r\n", key, value)
                    };
                    buf.push_str(&header);
                }
                
                buf
            };

            let mut is_request_line_send = false;
            loop {
                tokio::select! {
                    res = local_socket.read_buf(&mut local_buf) => {
                        let size = res?;
                        if size == 0 {
                            log::info!("local socket read buf: Socket was closed");
                            anyhow::bail!("Socket was closed");
                        }
                        let size = remote_socket.write_buf(&mut local_buf).await?;
                        if size == 0 {
                            log::info!("remote socket write buf: Socket was closed");
                            anyhow::bail!("Socket was closed");
                        }
                    }
                    res = remote_socket.read_buf(&mut remote_buf) => {
                        let size = res?;
                        if size == 0 {
                            log::info!("remote socket read buf: Socket was closed");
                            anyhow::bail!("Socket was closed");
                        }
                        log::info!("recv request: {}", String::from_utf8(remote_buf.to_vec()).unwrap());
                        request_parser.execute(&remote_buf[..])?;
                        remote_buf.clear();

                        let verson = request_parser.version();
                        if verson.is_some() && !is_request_line_send {
                            is_request_line_send = true;
                            let line = format!(
                                "{} {} HTTP/{}\r\n", 
                                request_parser.method().unwrap(), 
                                request_parser.url().unwrap(), 
                                request_parser.version().unwrap()
                            );
                            override_remote_buf.put(line.as_bytes());
                            let size = local_socket.write_buf(&mut override_remote_buf).await?;
                            log::info!("write line: {}", line);
                            if size == 0 {
                                return Err(anyhow::anyhow!("socket was closed"));
                            }
                            
                        }
                        let headers = request_parser.take_headers();
                        if !headers.is_empty() {
                            let headers = override_header_fn(&headers);
                            log::info!("override header: {headers}");
                            override_remote_buf.put(headers.as_bytes());
                            // for mut i in headers {
                            //     let headers = override_header_fn(&i);
                            //     log::info!("override header: {header}");
                            //     override_remote_buf.put(headers.as_bytes());
                            //     override_remote_buf.put("\r\n".as_bytes());
                            // }
                            let size = local_socket.write_buf(&mut override_remote_buf).await?;
                            if size == 0 {
                                return Err(anyhow::anyhow!("socket was closed"));
                            }

                        }

                        if request_parser.is_header_complete() {
                            override_remote_buf.put("\r\n".as_bytes());
                            let content = request_parser.take_body();
                            if !content.is_empty() {
                                override_remote_buf.put(content.as_slice());
                            }
                            request_parser.reset();
                            break;
                        }
                    }
                }
            }

            while !override_remote_buf.is_empty() {
                let size = local_socket.write_buf(&mut override_remote_buf).await?;
                if size == 0 {
                    return Err(anyhow::anyhow!("socket was closed"));
                }
            }

            while !local_buf.is_empty() {
                let size = remote_socket.write_buf(&mut local_buf).await?;
                if size == 0 {
                    return Err(anyhow::anyhow!("socket was closed"));
                }
            }
            
            log::info!("Header was handle finished!");
            tokio::io::copy_bidirectional(&mut local_socket, &mut remote_socket).await?;

            anyhow::Ok(())
        });
    }

    async fn accept_channel(&self, socket: &mut T::Socket) -> anyhow::Result<()> {
        let msg = JsonMessage::AcceptChannel {
            from: self.id,
            port: self.port,
            number: self.number,
        };
        let json = serde_json::to_vec(&msg)?;

        socket.write(&json).await?;

        anyhow::Ok(())
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
