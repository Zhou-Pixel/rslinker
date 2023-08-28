pub mod client;
pub mod protocol;
pub mod server;
pub mod utils;
pub mod config;

use serde:: {Deserialize, Serialize};

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
pub struct Args {
    #[arg(short, long, value_name="FILE")]
    pub config: Option<PathBuf>,
    pub client: bool
}

#[derive(Deserialize, Serialize)]
pub enum Message {
    Heartbeat,
    CreateChannel{
        number: i64,
        server_port: u16,
        channel_port: u16,
    },
}



