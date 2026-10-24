use serde::{Deserialize, Serialize};

pub const TCP_PORT: u16 = 29171;
pub const CHUNK_SIZE: usize = 262144; // 256KB
pub const MDNS_SERVICE_TYPE: &str = "_landrop._tcp.local.";

/// Peer sends `ack` frames after committing a unit that carried an `id`.
pub const CAP_ACK: &str = "ack";
/// Peer understands `chat` frames (edit/delete/react).
pub const CAP_CHAT: &str = "chat2";
/// Capabilities this build advertises in mDNS TXT (`caps=ack,chat2`) and identity.
pub const LOCAL_CAPS: &[&str] = &[CAP_ACK, CAP_CHAT];
/// Bounds applied to capability lists received from the network.
pub const MAX_CAPS: usize = 16;
pub const MAX_CAP_BYTES: usize = 32;

pub fn local_caps() -> Vec<String> {
    LOCAL_CAPS.iter().map(|cap| (*cap).to_string()).collect()
}

/// Parse a comma-separated TXT `caps` value, dropping empty and oversized
/// entries. A missing value (LanDrop 1.8.x) parses to an empty list.
pub fn parse_caps(raw: &str) -> Vec<String> {
    let mut caps: Vec<String> = Vec::new();
    for cap in raw.split(',').map(str::trim) {
        if cap.is_empty() || cap.len() > MAX_CAP_BYTES || caps.iter().any(|c| c == cap) {
            continue;
        }
        caps.push(cap.to_string());
        if caps.len() == MAX_CAPS {
            break;
        }
    }
    caps
}

/// Wire protocol v2. Every field added after 1.8.x is optional (or defaults)
/// and omitted when unset, so 1.8.x peers parse v2 frames and a v2 frame
/// without the new fields is byte-identical to its 1.8.x encoding.
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
        #[serde(default)]
        caps: Vec<String>,
    },
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
        /// Short text of the quoted message, so the quote still renders when
        /// the receiver lacks the original (pre-2.0 history, deleted locally).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_preview: Option<String>,
        /// Sender clock, unix milliseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sent_at: Option<u64>,
    },
    #[serde(rename = "file")]
    File { name: String, size: u64 },
    #[serde(rename = "dir")]
    Dir { name: String },
    #[serde(rename = "batch")]
    Batch {
        count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_preview: Option<String>,
    },
    #[serde(rename = "done")]
    Done,
    /// Receiver → sender after committing a unit that carried an `id`.
    #[serde(rename = "ack")]
    Ack {
        id: String,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Edit, delete or react to an earlier message. Only sent to `chat2` peers.
    /// `kind` stays a string on the wire so an unknown future kind is refused
    /// with an `ack{ok:false}` instead of failing to parse the frame.
    #[serde(rename = "chat")]
    Chat {
        kind: String,
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
    },
}

/// Chat event as accepted from the frontend (`lan_send_chat_event`) and as
/// interpreted from a received `chat` frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ChatEvent {
    Edit { id: String, text: String },
    Delete { id: String },
    React { id: String, emoji: Option<String> },
}

impl ChatEvent {
    pub fn id(&self) -> &str {
        match self {
            Self::Edit { id, .. } | Self::Delete { id } | Self::React { id, .. } => id,
        }
    }

    pub fn into_message(self) -> Message {
        match self {
            Self::Edit { id, text } => Message::Chat {
                kind: "edit".into(),
                id,
                text: Some(text),
                emoji: None,
            },
            Self::Delete { id } => Message::Chat {
                kind: "delete".into(),
                id,
                text: None,
                emoji: None,
            },
            Self::React { id, emoji } => Message::Chat {
                kind: "react".into(),
                id,
                text: None,
                emoji,
            },
        }
    }

    /// Interpret a received `chat` frame; `None` for an unknown or malformed kind.
    pub fn from_frame(
        kind: &str,
        id: String,
        text: Option<String>,
        emoji: Option<String>,
    ) -> Option<Self> {
        match kind {
            "edit" => Some(Self::Edit { id, text: text? }),
            "delete" => Some(Self::Delete { id }),
            "react" => Some(Self::React { id, emoji }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_caps, ChatEvent, Message};

    #[test]
    fn legacy_1_8_frames_still_parse() {
        let frames = [
            r#"{"type":"discover"}"#,
            r#"{"type":"identity","app":"LanDrop","alias":"Old","device_type":"desktop"}"#,
            r#"{"type":"text","text":"hi"}"#,
            r#"{"type":"file","name":"a.txt","size":3}"#,
            r#"{"type":"dir","name":"docs"}"#,
            r#"{"type":"batch","count":2}"#,
            r#"{"type":"done"}"#,
        ];
        for frame in frames {
            serde_json::from_str::<Message>(frame).unwrap_or_else(|e| panic!("{frame}: {e}"));
        }
        let Message::Identity { caps, .. } = serde_json::from_str(frames[1]).unwrap() else {
            panic!("expected identity");
        };
        assert!(caps.is_empty(), "1.8.x identity has no capabilities");
        let Message::Text {
            id,
            reply_to,
            sent_at,
            ..
        } = serde_json::from_str(frames[2]).unwrap()
        else {
            panic!("expected text");
        };
        assert_eq!((id, reply_to, sent_at), (None, None, None));
        let Message::Batch {
            count,
            id,
            reply_to,
            reply_preview,
        } = serde_json::from_str(frames[5]).unwrap()
        else {
            panic!("expected batch");
        };
        assert_eq!((count, id, reply_to, reply_preview), (2, None, None, None));
    }

    #[test]
    fn v2_frames_without_new_fields_are_byte_identical_to_1_8() {
        let text = Message::Text {
            text: "hi".into(),
            id: None,
            reply_to: None,
            reply_preview: None,
            sent_at: None,
        };
        assert_eq!(
            serde_json::to_string(&text).unwrap(),
            r#"{"type":"text","text":"hi"}"#
        );
        let batch = Message::Batch {
            count: 2,
            id: None,
            reply_to: None,
            reply_preview: None,
        };
        assert_eq!(
            serde_json::to_string(&batch).unwrap(),
            r#"{"type":"batch","count":2}"#
        );
    }

    #[test]
    fn v2_fields_round_trip_and_unknown_fields_are_ignored() {
        let text = Message::Text {
            text: "hi".into(),
            id: Some("m1".into()),
            reply_to: Some("m0".into()),
            reply_preview: Some("earlier text".into()),
            sent_at: Some(1_700_000_000_000),
        };
        let json = serde_json::to_string(&text).unwrap();
        assert_eq!(
            json,
            r#"{"type":"text","text":"hi","id":"m1","reply_to":"m0","reply_preview":"earlier text","sent_at":1700000000000}"#
        );
        // A future peer may add more fields; they must not break parsing.
        let future = r#"{"type":"ack","id":"m1","ok":false,"error":"disk full","extra":1}"#;
        let Message::Ack { id, ok, error } = serde_json::from_str(future).unwrap() else {
            panic!("expected ack");
        };
        assert_eq!(
            (id.as_str(), ok, error.as_deref()),
            ("m1", false, Some("disk full"))
        );
        let chat = r#"{"type":"chat","kind":"react","id":"m1","emoji":"👍"}"#;
        let Message::Chat {
            kind,
            id,
            text,
            emoji,
        } = serde_json::from_str(chat).unwrap()
        else {
            panic!("expected chat");
        };
        assert_eq!(
            ChatEvent::from_frame(&kind, id, text, emoji),
            Some(ChatEvent::React {
                id: "m1".into(),
                emoji: Some("👍".into())
            })
        );
    }

    #[test]
    fn chat_events_deserialize_from_the_frontend_shape() {
        let edit: ChatEvent =
            serde_json::from_str(r#"{"kind":"edit","id":"m1","text":"fixed"}"#).unwrap();
        assert_eq!(
            edit,
            ChatEvent::Edit {
                id: "m1".into(),
                text: "fixed".into()
            }
        );
        let unreact: ChatEvent =
            serde_json::from_str(r#"{"kind":"react","id":"m1","emoji":null}"#).unwrap();
        assert_eq!(
            unreact,
            ChatEvent::React {
                id: "m1".into(),
                emoji: None
            }
        );
        assert!(ChatEvent::from_frame("pin", "m1".into(), None, None).is_none());
        assert!(ChatEvent::from_frame("edit", "m1".into(), None, None).is_none());
    }

    #[test]
    fn parses_bounded_capability_lists() {
        assert_eq!(parse_caps("ack,chat2"), vec!["ack", "chat2"]);
        assert_eq!(parse_caps(" ack , ,ack,chat2 "), vec!["ack", "chat2"]);
        assert!(parse_caps("").is_empty());
        assert!(parse_caps(&"x".repeat(64)).is_empty());
        assert_eq!(parse_caps(&vec!["a"; 40].join(",")).len(), 1);
        let many: Vec<String> = (0..40).map(|i| format!("c{i}")).collect();
        assert_eq!(parse_caps(&many.join(",")).len(), super::MAX_CAPS);
    }
}
