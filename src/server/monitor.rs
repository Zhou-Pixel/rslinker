use crate::protocol::udp::{UdpClient, UdpServer};
use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};

use crate::{protocol::Port, utils::chat};

pub enum ChannelMessage {
    NewTcp(TcpStream),
    NewUdp(UdpClient),
    Error,
}


pub struct Udp {
    addr: SocketAddr,
    employee: chat::Employee<(), ChannelMessage, Port>,
}

impl Udp {
    pub fn new(addr: SocketAddr, employee: chat::Employee<(), ChannelMessage, Port>) -> Self {
        Self { addr, employee }
    }
    pub fn run(mut self) {
        tokio::spawn(async move {
            let mut server = UdpServer::bind(self.addr).await?;
            loop {
                tokio::select! {
                    client = server.accept() => {
                        self.employee.report(ChannelMessage::NewUdp(client))?;
                    }
                    _ = self.employee.wait() => {
                        break;
                    }
                }
            }
            anyhow::Ok(())
        });
    }
}

pub struct Tcp {
    addr: SocketAddr,
    employee: chat::Employee<(), ChannelMessage, Port>,
}

impl Tcp {
    pub fn new(addr: SocketAddr, employee: chat::Employee<(), ChannelMessage, Port>) -> Self {
        Self { addr, employee }
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            let result = async {
                let listener = TcpListener::bind(self.addr).await?;
                loop {
                    tokio::select! {
                        result = listener.accept() => {
                            let (socket, _)  = result?;
                            self.employee.report(ChannelMessage::NewTcp(socket))?;
                        }
                        _ = self.employee.wait() => {
                            return anyhow::Ok(());
                        }
                    }
                }
            }
            .await;
            if result.is_err() {
                self.employee.report(ChannelMessage::Error)?;
            }
            anyhow::Ok(())
        });
    }
}
