use crate::protocol::udp::UdpServer;
use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::{protocol::Port, utils::chat};
use super::controller::Message;

pub struct Udp {
    port: Port,
    employee: chat::Employee<(), Message, Port>,
}

impl Udp {
    pub fn new(port: Port, employee: chat::Employee<(), Message, Port>) -> Self {
        Self {
            port,
            employee
        }
    }
    pub fn run(mut self) {
        tokio::spawn(async move {
            let addr: SocketAddr = format!("0.0.0.0:{}", self.port.port).parse().unwrap();
            let mut server = UdpServer::bind(addr).await?;
            loop {
                tokio::select! {
                    client = server.accept() => {
                        self.employee.report(Message::NewUdp(client))?;
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
    port: Port,
    employee: chat::Employee<(), Message, Port>,
}

impl Tcp {
    pub fn new(
        port: Port,
        employee: chat::Employee<(), Message, Port>,
    ) -> Self {
        Self { port, employee }
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            let result = async {
                let addr: SocketAddr = format!("0.0.0.0:{}", self.port.port).parse().unwrap();
                let listener = TcpListener::bind(addr).await?;
                loop {
                    tokio::select! {
                        result = listener.accept() => {
                            let (socket, _)  = result?;
                            self.employee.report(Message::NewTcp(socket))?;
                        }
                        _ = self.employee.wait() => {
                            return anyhow::Ok(());
                        }
                    }
                }
            }
            .await;
            if result.is_err() {
                self.employee.report(Message::Error)?;
            }
            anyhow::Ok(())
        });
    }
}