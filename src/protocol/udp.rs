use fast_async_mutex::RwLock;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

pub struct UdpServer {
    // table: HashMap<SocketAddr, UnboundedSender<Vec<u8>>>,
    recver: UnboundedReceiver<UdpClient>,
    shutdown: UnboundedSender<()>,
}

pub struct UdpClient {
    addr: SocketAddr,
    recver: UnboundedReceiver<Vec<u8>>,
    socket: Arc<UdpSocket>,
    table: Arc<RwLock<HashMap<SocketAddr, UnboundedSender<Vec<u8>>>>>,
}

impl Drop for UdpServer {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}

impl Drop for UdpClient {
    fn drop(&mut self) {
        let table = Arc::clone(&self.table);
        let addr = self.addr;
        tokio::spawn(async move {
            let mut write_lock = table.write().await;
            write_lock.remove(&addr);
        });
    }
}

impl UdpServer {
    pub async fn bind(addr: SocketAddr) -> anyhow::Result<Self> {
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let cloned = socket.clone();
        let (sender, recver) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let table: HashMap<SocketAddr, UnboundedSender<Vec<u8>>> = HashMap::new();
            let table = Arc::new(RwLock::new(table));
            loop {
                let mut buf = vec![];
                tokio::select! {
                    result = cloned.recv_buf_from(&mut buf) => {
                        let (size, addr) = result?;
                        let mut write_lock = table.write().await;
                        if write_lock.contains_key(&addr) {
                            if write_lock.get(&addr).unwrap().send(buf[..size].to_vec()).is_ok() {
                                continue;
                            }
                        }
                        let (tx, rx) = mpsc::unbounded_channel();
                        tx.send(buf[..size].to_vec()).unwrap();
                        write_lock.insert(addr, tx);
                        sender
                            .send(UdpClient {
                                addr,
                                recver: rx,
                                socket: cloned.clone(),
                                table: Arc::clone(&table)
                            })
                            .unwrap();
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
            anyhow::Ok(())
        });
        Ok(Self {
            // table: Default::default(),
            recver,
            shutdown: shutdown_tx,
        })
    }

    pub async fn accept(&mut self) -> UdpClient {
        self.recver.recv().await.unwrap()
    }
}

impl UdpClient {
    pub async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        Ok(self.recver.recv().await.unwrap())
    }

    pub async fn write(&self, data: &[u8]) -> anyhow::Result<usize> {
        Ok(self.socket.send_to(data, self.addr).await?)
    }
}
