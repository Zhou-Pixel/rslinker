use std::path::Path;
use toml::{map::Map, Value};

pub mod client;
pub mod server;

fn override_config(first: &mut Map<String, Value>, second: &Map<String, Value>) {
    for (key, value) in second {
        match (first.get_mut(key), value) {
            (None, _) => {
                first.insert(key.to_owned(), value.to_owned());
            }
            // (Some(Value::Table(first)), Value::Table(second)) => {
            //     override_config(first, second)
            // },
            _ => {}
        }
    }
}

fn link_config(table: &mut Map<String, Value>, configs: &Map<String, Value>) -> anyhow::Result<()> {
    for (_, value) in table.iter_mut() {
        if let Value::Table(value) = value {
            link_config(value, configs)?;
        }
    }
    if let Some(config) = table.remove("redirect") {
        let config = config
            .as_str()
            .ok_or(anyhow::anyhow!("config must be str"))?;
        match configs.get(config) {
            Some(config) => {
                let config = config
                    .as_table()
                    .ok_or(anyhow::anyhow!("config must be table"))?;
                override_config(table, config);
                // for (key, value) in config {

                //     match table.get_mut(key) {
                //         Some(Value::Table(value_table)) => {

                //         },
                //         None => {
                //             table.insert(key.to_owned(), value.to_owned());
                //         },
                //         _ => {}
                //     }

                // }
            }
            None => anyhow::bail!("Can't find the config named {config:?}"),
        }
    }

    Ok(())
}

async fn merge_config(path: impl AsRef<Path>) -> anyhow::Result<Map<String, Value>> {
    let string = tokio::fs::read(path).await?;

    let string = String::from_utf8(string)?;

    let mut table: Map<String, Value> = toml::from_str(&string)?;

    let configs = Value::Table(Map::new());
    let configs = table.remove("config").unwrap_or(configs);
    let configs = configs.as_table()
        .ok_or(anyhow::anyhow!("config must be table"))?;

    for (_, value) in &mut table {
        if let Value::Table(value) = value {
            link_config(value, configs)?;
        }
    }

    Ok(table)
}

pub async fn make_client_config(path: impl AsRef<Path>) -> anyhow::Result<client::MultipleClient> {
    Ok(merge_config(path).await?.try_into()?)
}

pub async fn make_server_config(path: impl AsRef<Path>) -> anyhow::Result<server::SingleServer> {
    Ok(merge_config(path).await?.try_into()?)
}

// #[cfg(test)]
// mod test {
//     use std::fs;

//     #[test]
//     fn read_client_config_toml() -> anyhow::Result<()> {
//         let data = fs::read("./client.toml")?;
//         let client = super::_client::Client::from_str(String::from_utf8(data)?.as_str())?;

//         println!("client: {client:?}");
//         Ok(())
//     }

//     #[test]
//     fn read_server_config_toml() -> anyhow::Result<()> {
//         let data = fs::read("./server.toml")?;
//         let server = super::_server::Server::from_str(String::from_utf8(data)?.as_str())?;

//         println!("client: {server:?}");
//         Ok(())
//     }
// }

// pub mod _server {

//     use std::net::SocketAddr;

//     use toml::{map::Map, Value};

//     #[derive(Debug)]
//     pub enum Encrypted {
//         EnableClientAuth {
//             cert: String,
//             key: String,
//             ca: String,
//         },
//         DisableClientAuth {
//             cert: String,
//             key: String,
//         },
//     }

//     #[derive(Debug)]
//     pub struct Server {
//         pub addr: SocketAddr,
//         pub protocol: ControllProtocol,
//     }

//     impl Server {
//         pub fn from_str(s: &str) -> anyhow::Result<Self> {
//             let mut value: Value = toml::from_str(s)?;

//             let table = value
//                 .as_table_mut()
//                 .ok_or(anyhow::anyhow!("The Global element must be an element"))?;

//             let configs = table
//                 .remove("config")
//                 .unwrap_or(Value::Table(Default::default()));

//             let configs = configs
//                 .as_table()
//                 .ok_or(anyhow::anyhow!("config must be table"))?;

//             let server = table
//                 .get_mut("server")
//                 .ok_or(anyhow::anyhow!("Can't find server"))?
//                 .as_table_mut()
//                 .ok_or(anyhow::anyhow!("server must be table"))?;

//             super::link_config(server, configs)?;

//             let protocol: ControllProtocol = (server as &Map<_, _>).try_into()?;

//             let addr = Value::String("0.0.0.0".to_string());
//             let addr = server
//                 .get("addr")
//                 .unwrap_or(&addr)
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("addr must be str"))?;

//             let port = server
//                 .get("port")
//                 .ok_or(anyhow::anyhow!("Can't find port"))?
//                 .as_integer()
//                 .ok_or(anyhow::anyhow!("port must be integer"))?;

//             let addr: SocketAddr = format!("{}:{}", addr, port).parse()?;

//             Ok(Server { addr, protocol })
//         }
//     }

//     impl TryFrom<&Map<String, Value>> for Encrypted {
//         type Error = anyhow::Error;

//         fn try_from(value: &Map<String, Value>) -> Result<Self, Self::Error> {
//             let enable_client_auth = value
//                 .get("enable_client_auth")
//                 .unwrap_or(&Value::Boolean(false))
//                 .as_bool()
//                 .ok_or(anyhow::anyhow!("enable_client_auth must be bool"))?;
//             let cert = value
//                 .get("cert")
//                 .ok_or(anyhow::anyhow!("Can't not find cert"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("cert must be str"))?;
//             let key = value
//                 .get("key")
//                 .ok_or(anyhow::anyhow!("Can't not find key"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("key must be str"))?;
//             Ok(if enable_client_auth {
//                 let ca = value
//                     .get("ca")
//                     .ok_or(anyhow::anyhow!("Can't not find ca"))?
//                     .as_str()
//                     .ok_or(anyhow::anyhow!("ca must be str"))?;
//                 Self::EnableClientAuth {
//                     cert: cert.to_string(),
//                     key: key.to_string(),
//                     ca: ca.to_string(),
//                 }
//             } else {
//                 Self::DisableClientAuth {
//                     cert: cert.to_string(),
//                     key: key.to_string(),
//                 }
//             })
//         }
//     }

//     #[derive(Debug)]
//     pub enum ControllProtocol {
//         Tcp { nodelay: bool },
//         Kcp,
//         Quic(Encrypted),
//         Tls(Encrypted),
//     }

//     impl TryFrom<&Map<String, Value>> for ControllProtocol {
//         type Error = anyhow::Error;

//         fn try_from(value: &Map<String, Value>) -> Result<Self, Self::Error> {
//             let protocol = value
//                 .get("protocol")
//                 .ok_or(anyhow::anyhow!("Can't find protocol"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("protocol must be str"))?;

//             Ok(match protocol {
//                 "quic" => Self::Quic(value.try_into()?),
//                 "tls" => Self::Tls(value.try_into()?),
//                 "tcp" => Self::Tcp {
//                     nodelay: value
//                         .get("nodelay")
//                         .unwrap_or(&Value::Boolean(true))
//                         .as_bool()
//                         .ok_or(anyhow::anyhow!("nodelay must be bool"))?,
//                 },
//                 "kcp" => Self::Kcp,
//                 _ => anyhow::bail!("Not support protocol: {protocol}"),
//             })
//         }
//     }
// }

// pub mod _client {
//     use std::{collections::HashMap, net::SocketAddr};

//     use toml::{map::Map, Value};

//     use super::{client, link_config};

//     #[derive(Debug)]
//     pub enum Encrypted {
//         EnableClientAuth {
//             cert: String,
//             key: String,
//             ca: String,
//             server_name: String,
//         },
//         DisableClientAuth {
//             ca: String,
//             server_name: String,
//         },
//     }

//     impl TryFrom<&Map<String, Value>> for Encrypted {
//         type Error = anyhow::Error;

//         fn try_from(value: &Map<String, Value>) -> Result<Self, Self::Error> {
//             let server_name = value
//                 .get("server_name")
//                 .ok_or(anyhow::anyhow!("Can't not find server_name"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("server_name must be str"))?;
//             let enable_client_auth = value
//                 .get("enable_client_auth")
//                 .unwrap_or(&Value::Boolean(false))
//                 .as_bool()
//                 .ok_or(anyhow::anyhow!("enable_client_auth must be bool"))?;
//             let ca = value
//                 .get("ca")
//                 .ok_or(anyhow::anyhow!("Can't not find ca"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("ca must be str"))?;
//             Ok(if enable_client_auth {
//                 let cert = value
//                     .get("cert")
//                     .ok_or(anyhow::anyhow!("Can't not find cert"))?
//                     .as_str()
//                     .ok_or(anyhow::anyhow!("cert must be str"))?;
//                 let key = value
//                     .get("key")
//                     .ok_or(anyhow::anyhow!("Can't not find key"))?
//                     .as_str()
//                     .ok_or(anyhow::anyhow!("key must be str"))?;
//                 Self::EnableClientAuth {
//                     cert: cert.to_string(),
//                     key: key.to_string(),
//                     ca: ca.to_string(),
//                     server_name: server_name.to_string(),
//                 }
//             } else {
//                 Self::DisableClientAuth {
//                     ca: ca.to_string(),
//                     server_name: server_name.to_string(),
//                 }
//             })
//         }
//     }

//     #[derive(Debug)]
//     pub enum ControllProtocol {
//         Tcp { nodelay: bool },
//         Kcp,
//         Quic(Encrypted),
//         Tls(Encrypted),
//     }

//     impl TryFrom<&Map<String, Value>> for ControllProtocol {
//         type Error = anyhow::Error;

//         fn try_from(value: &Map<String, Value>) -> Result<Self, Self::Error> {
//             let protocol = value
//                 .get("protocol")
//                 .ok_or(anyhow::anyhow!("Can't find protocol"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("protocol must be str"))?;

//             Ok(match protocol {
//                 "quic" => Self::Quic(value.try_into()?),
//                 "tls" => Self::Tls(value.try_into()?),
//                 "tcp" => Self::Tcp {
//                     nodelay: value
//                         .get("nodelay")
//                         .unwrap_or(&Value::Boolean(true))
//                         .as_bool()
//                         .ok_or(anyhow::anyhow!("nodelay must be bool"))?,
//                 },
//                 "kcp" => Self::Kcp,
//                 _ => anyhow::bail!("Not support protocol: {protocol}"),
//             })
//         }
//     }

//     pub struct Configuration {
//         clients: Vec<Client>,
//     }

//     #[derive(Debug)]
//     pub struct Client {
//         server_addr: SocketAddr,
//         accept_conflict: bool,
//         retry_times: u32,
//         heartbeat_interval: Option<u64>,
//         protocol: ControllProtocol,
//         links: Vec<Link>,
//     }

//     impl Client {
//         pub fn from_str(s: &str) -> anyhow::Result<Vec<Self>> {
//             let mut value: Value = toml::from_str(s)?;

//             let table = value
//                 .as_table_mut()
//                 .ok_or(anyhow::anyhow!("The Global element must be an element"))?;

//             let configs = table
//                 .remove("config")
//                 .unwrap_or(Value::Table(Default::default()));

//             let configs = configs
//                 .as_table()
//                 .ok_or(anyhow::anyhow!("config must be table"))?;

//             let clients = table
//                 .get_mut("client")
//                 .ok_or(anyhow::anyhow!("Can't not find any clients"))?;

//             let clients = match clients {
//                 Value::Array(clients) => {
//                     let mut vec = vec![];
//                     for i in clients {
//                         let client = i
//                             .as_table_mut()
//                             .ok_or(anyhow::anyhow!("Clietn must be table"))?;
//                         vec.push(client);
//                     }
//                     vec
//                 }
//                 Value::Table(client) => vec![client],
//                 _ => anyhow::bail!("Client must be table(s)"),
//             };

//             let mut results = vec![];

//             for i in clients {
//                 let client = i;

//                 link_config(client, configs)?;

//                 let protocol = (client as &Map<_, _>).try_into()?;

//                 let server_addr = client
//                     .get("server_addr")
//                     .ok_or(anyhow::anyhow!("Can't not find server_addr"))?
//                     .as_str()
//                     .ok_or(anyhow::anyhow!("server_addr must be str"))?;

//                 let server_port = client
//                     .get("server_port")
//                     .ok_or(anyhow::anyhow!("Can't not find server_addr"))?
//                     .as_integer()
//                     .ok_or(anyhow::anyhow!("server_port must be int"))?;

//                 let addr = format!("{}:{}", server_addr, server_port);

//                 let addr: SocketAddr = addr
//                     .parse()
//                     .map_err(|e| anyhow::anyhow!("Invalid addr: {addr}\n, err: {e}"))?;

//                 let accept_conflict = match client.get("accept_conflict") {
//                     Some(Value::Boolean(accept)) => *accept,
//                     Some(_) => anyhow::bail!("accept_conflict must be bool"),
//                     None => true,
//                 };

//                 let retry_times = match client.get("retry_times") {
//                     Some(Value::Integer(retry_times)) if *retry_times >= 0 => *retry_times as u32,
//                     Some(_) => anyhow::bail!("retry_times must be int and >= 0"),
//                     None => 0,
//                 };
//                 let heartbeat_interval = match client.get("heartbeat_interval") {
//                     Some(Value::Integer(value)) if *value >= 0 => Some(*value as u64),
//                     Some(_) => anyhow::bail!("heartbeat_interval must be int and >= 0"),
//                     None => None,
//                 };
//                 let mut links = Value::Array(vec![]);
//                 let links = client
//                     .get_mut("link")
//                     .unwrap_or(&mut links)
//                     .as_array_mut()
//                     .ok_or(anyhow::anyhow!("Parse link failed"))?;

//                 let mut res_links = vec![];
//                 for i in links {
//                     let link = i
//                         .as_table_mut()
//                         .ok_or(anyhow::anyhow!("Parse link failed"))?;
//                     link_config(link, configs)?;
//                     let link: Link = (link as &Map<_, _>).try_into()?;
//                     res_links.push(link);
//                 }
//                 results.push(Client {
//                     server_addr: addr,
//                     accept_conflict,
//                     retry_times,
//                     heartbeat_interval,
//                     protocol,
//                     links: res_links,
//                 });
//             }

//             Ok(results)
//         }
//     }

//     #[derive(Debug)]
//     pub struct Link {
//         remote_port: u16,
//         local_addr: SocketAddr,
//         protocol: TransferProtocol,
//     }

//     impl TryFrom<&Map<String, Value>> for Link {
//         type Error = anyhow::Error;

//         fn try_from(value: &Map<String, Value>) -> Result<Self, Self::Error> {
//             let remote_port = value
//                 .get("remote_port")
//                 .ok_or(anyhow::anyhow!("Can't find remote_port"))?
//                 .as_integer()
//                 .ok_or(anyhow::anyhow!("remote_port must be int"))?;

//             let remote_port = if remote_port > u16::MAX as i64 || remote_port < 0 {
//                 anyhow::bail!("Invalid port: {remote_port}");
//             } else {
//                 remote_port as u16
//             };

//             let local_addr = value
//                 .get("local_addr")
//                 .ok_or(anyhow::anyhow!("Can't find local_addr"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("local_addr must be str"))?;

//             let local_port = value
//                 .get("local_port")
//                 .ok_or(anyhow::anyhow!("Can't find local_port"))?
//                 .as_integer()
//                 .ok_or(anyhow::anyhow!("local_port must be int"))?;

//             let local_port = if local_port > u16::MAX as i64 || local_port < 0 {
//                 anyhow::bail!("Invalid port: {local_port}");
//             } else {
//                 local_port as u16
//             };

//             let addr = format!("{}:{}", local_addr, local_port);
//             let addr: SocketAddr = addr
//                 .parse()
//                 .map_err(|e| anyhow::anyhow!("Invalid local address: {addr}, err: {e}"))?;

//             let protocol: TransferProtocol = value.try_into()?;

//             Ok(Self {
//                 remote_port,
//                 local_addr: addr,
//                 protocol,
//             })
//         }
//     }

//     #[derive(Debug)]
//     pub enum TransferProtocol {
//         Http { headers: HashMap<String, String> },
//         Udp,
//         Tcp,
//     }

//     impl TryFrom<&Map<String, Value>> for TransferProtocol {
//         type Error = anyhow::Error;

//         fn try_from(value: &Map<String, Value>) -> Result<Self, Self::Error> {
//             let protocol = value
//                 .get("protocol")
//                 .ok_or(anyhow::anyhow!("Can't find protocol"))?
//                 .as_str()
//                 .ok_or(anyhow::anyhow!("protocol must be str"))?;
//             //  {
//             //    Some(Value::String(protocol)) => protocol,
//             //    Some(_) => anyhow::bail!("protocol must be str"),
//             //    None => anyhow::bail!("Can't not find protocol"),
//             // };

//             Ok(match protocol {
//                 "udp" => Self::Udp,
//                 "tcp" => Self::Tcp,
//                 "http" => {
//                     let default_value = Value::Table(Map::new());
//                     let r#override = value
//                         .get("override")
//                         .unwrap_or(&default_value)
//                         .as_table()
//                         .ok_or(anyhow::anyhow!(
//                             r#"Http override header example: Authorization = "bear xxx""#
//                         ))?;
//                     let mut headers = HashMap::new();
//                     for (key, value) in r#override {
//                         let Value::String(value) = value else {
//                             anyhow::bail!(
//                                 r#"Http override header example: Authorization = "bear xxx""#
//                             );
//                         };
//                         headers.insert(key.to_string(), value.to_string());
//                     }

//                     Self::Http { headers }
//                 }
//                 _ => anyhow::bail!("Not support protocol: {protocol}"),
//             })
//         }
//     }
// }
