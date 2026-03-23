use serde::{Deserialize, Serialize};

pub const BEACON_PORT: u16 = 29170;
pub const TCP_PORT: u16 = 29171;
pub const BEACON_MAGIC: &[u8; 8] = b"LANDROP1";
pub const CHUNK_SIZE: usize = 262144; // 256KB

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "file")]
    File { name: String, size: u64 },
    #[serde(rename = "dir")]
    Dir { name: String },
    #[serde(rename = "batch")]
    Batch { count: u32 },
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
}
