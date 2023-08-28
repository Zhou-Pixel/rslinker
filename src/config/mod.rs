use serde::{ Deserialize, Serialize };

pub mod client;
pub mod server;








// #[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
// pub enum MiddleProtocol {
//     #[serde(rename="tcp")]
//     Tcp,
//     #[serde(rename="quic")]
//     Quic
// }