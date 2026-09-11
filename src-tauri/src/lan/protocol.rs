use serde::{Deserialize, Serialize};

pub const TCP_PORT: u16 = 29171;
pub const CHUNK_SIZE: usize = 262144; // 256KB
pub const MDNS_SERVICE_TYPE: &str = "_landrop._tcp.local.";

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "discover")]
    Discover,
    #[serde(rename = "identity")]
    Identity {
        app: String,
        alias: String,
        device_type: String,
    },
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
}
