//! Native notifications are navigation requests, never commands to open a file,
//! follow an arbitrary URL, or install an update. Windows protocol activation
//! survives a stopped process; a bounded queue bridges WebView startup/reload.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

const ACTIVATION_EVENT: &str = "native-notification-activated";
const MAX_PENDING_ACTIVATIONS: usize = 32;
const SCHEME: &str = "landrop-notification://";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum NotificationTarget {
    Peer {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    Updates {},
}

impl NotificationTarget {
    fn validated(self) -> Result<Self, String> {
        match self {
            Self::Peer { peer_id } => {
                let id = uuid::Uuid::parse_str(&peer_id)
                    .map_err(|_| "Notification peer ID must be a UUID".to_owned())?;
                if id.is_nil() {
                    return Err("Notification peer ID must not be empty".to_owned());
                }
                Ok(Self::Peer {
                    peer_id: id.hyphenated().to_string(),
                })
            }
            Self::Updates {} => Ok(Self::Updates {}),
        }
    }

    #[cfg(any(windows, test))]
    fn uri(&self) -> String {
        match self {
            Self::Peer { peer_id } => format!("{SCHEME}peer/{peer_id}"),
            // Windows Foundation.Uri canonicalizes a host-only URI to a slash.
            Self::Updates {} => format!("{SCHEME}updates/"),
        }
    }

    fn from_uri(uri: &str) -> Option<Self> {
        if uri == format!("{SCHEME}updates/") {
            return Some(Self::Updates {});
        }
        let peer_id = uri.strip_prefix(&format!("{SCHEME}peer/"))?;
        let target = Self::Peer {
            peer_id: peer_id.to_owned(),
        }
        .validated()
        .ok()?;
        // Deliberately no URL decoding, queries, fragments, extra arguments, or
        // permissive UUID variants in shell input. Only the URI we generate.
        if let Self::Peer { peer_id: canonical } = &target {
            (peer_id == canonical).then_some(target)
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeNotification {
    title: String,
    body: String,
    target: NotificationTarget,
    #[serde(default)]
    silent: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NotificationActivation {
    id: String,
    target: NotificationTarget,
}

#[derive(Default)]
pub struct NotificationState {
    pending: Mutex<VecDeque<NotificationActivation>>,
}

impl NotificationState {
    fn enqueue(&self, target: NotificationTarget) -> Option<NotificationActivation> {
        let mut queue = self.pending.lock().ok()?;
        let activation = NotificationActivation {
            id: uuid::Uuid::new_v4().to_string(),
            target,
        };
        if queue.len() == MAX_PENDING_ACTIVATIONS {
            queue.pop_front();
        }
        queue.push_back(activation.clone());
        Some(activation)
    }
}

pub fn handle_launch_args(app: &AppHandle, args: impl IntoIterator<Item = String>) {
    let Some(state) = app.try_state::<NotificationState>() else {
        return;
    };
    // Shell/second-instance arguments are untrusted. One activation per launch.
    if let Some(target) = args
        .into_iter()
        .find_map(|arg| NotificationTarget::from_uri(&arg))
    {
        if let Some(activation) = state.enqueue(target) {
            let _ = app.emit(ACTIVATION_EVENT, activation);
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

/// Listen first, then drain; de-duplicate the event/drain overlap by activation ID.
#[tauri::command]
pub fn take_notification_activations(
    state: State<'_, NotificationState>,
) -> Result<Vec<NotificationActivation>, String> {
    state
        .pending
        .lock()
        .map(|mut queue| queue.drain(..).collect())
        .map_err(|_| "Notification activation queue unavailable".to_owned())
}

#[tauri::command]
pub async fn show_native_notification(
    app: AppHandle,
    notification: NativeNotification,
) -> Result<(), String> {
    let notification = NativeNotification {
        title: notification_text(&notification.title, 120),
        body: notification_text(&notification.body, 320),
        target: notification.target.validated()?,
        silent: notification.silent,
    };
    if notification.title.is_empty() {
        return Err("Notification title must not be empty".to_owned());
    }
    #[cfg(windows)]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        let handle = app.clone();
        // WinRT and Shell COM calls use Tauri's initialized UI apartment.
        app.run_on_main_thread(move || {
            let _ = send.send(windows_native::show(&handle, &notification));
        })
        .map_err(|error| error.to_string())?;
        receive
            .await
            .map_err(|_| "Notification dispatcher stopped".to_owned())?
    }
    #[cfg(not(windows))]
    {
        // The frontend uses the permission-aware Tauri plugin on other systems.
        let _ = (app, notification);
        Err("Actionable native notifications are supported on Windows".to_owned())
    }
}

fn notification_text(input: &str, limit: usize) -> String {
    input
        .chars()
        .filter(|&ch| {
            (ch == '\n' || ch == '\t' || !ch.is_control())
                && !matches!(ch, '\u{fffe}' | '\u{ffff}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(any(windows, test))]
fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(any(windows, test))]
fn toast_xml(notification: &NativeNotification, icon_uri: &str) -> String {
    let uri = xml_escape(&notification.target.uri());
    let action_label = match notification.target {
        NotificationTarget::Peer { .. } => "Open conversation",
        NotificationTarget::Updates {} => "View update",
    };
    let audio = if notification.silent {
        "<audio silent=\"true\"/>"
    } else {
        ""
    };
    format!(
        "<toast activationType=\"protocol\" launch=\"{uri}\"><visual><binding template=\"ToastGeneric\"><image placement=\"appLogoOverride\" src=\"{}\" alt=\"LanDrop\"/><text>{}</text><text>{}</text></binding></visual><actions><action content=\"{action_label}\" activationType=\"protocol\" arguments=\"{uri}\"/></actions>{audio}</toast>",
        xml_escape(icon_uri), xml_escape(&notification.title), xml_escape(&notification.body)
    )
}

#[cfg(windows)]
mod windows_native;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "test fixtures must fail loudly on invalid data"
)]
mod tests {
    use super::*;

    const PEER_ID: &str = "02b54e69-ec9a-422f-b5a3-5f906348b439";

    #[test]
    fn round_trips_only_known_navigation_targets() {
        for target in [
            NotificationTarget::Updates {},
            NotificationTarget::Peer {
                peer_id: PEER_ID.into(),
            },
        ] {
            assert_eq!(NotificationTarget::from_uri(&target.uri()), Some(target));
        }
    }

    #[test]
    fn rejects_shell_and_uri_injection() {
        for uri in [
            "https://example.com",
            "file:///C:/test.exe",
            "landrop-notification://updates?install=true",
            "landrop-notification://updates#payload",
            "landrop-notification://updates/?install=true",
            "landrop-notification://updates/#payload",
            "landrop-notification://peer/00000000-0000-0000-0000-000000000000",
            "landrop-notification://peer/../../secret",
            "landrop-notification://peer/%30%32b54e69-ec9a-422f-b5a3-5f906348b439",
            "landrop-notification://peer/02b54e69-ec9a-422f-b5a3-5f906348b439 --open evil.exe",
            "landrop-notification://peer/02b54e69-ec9a-422f-b5a3-5f906348b439/",
        ] {
            assert_eq!(NotificationTarget::from_uri(uri), None, "{uri}");
        }
    }

    #[test]
    fn serializes_the_frontend_contract() {
        let target = NotificationTarget::Peer {
            peer_id: PEER_ID.into(),
        };
        assert_eq!(
            serde_json::to_value(target).unwrap(),
            serde_json::json!({"kind":"peer", "peerId":PEER_ID})
        );
        assert!(serde_json::from_value::<NotificationTarget>(
            serde_json::json!({"kind":"updates", "url":"https://evil.test"})
        )
        .is_err());
    }

    #[test]
    fn escapes_remote_text_and_icon_attributes() {
        let notification = NativeNotification {
            title: "<action arguments=\"evil\">&'".into(),
            body: "Hello <world>".into(),
            target: NotificationTarget::Updates {},
            silent: true,
        };
        let xml = toast_xml(&notification, "file:///C:/icon&a\".png");
        assert!(xml.contains("&lt;action arguments=&quot;evil&quot;&gt;&amp;&apos;"));
        assert!(xml.contains("file:///C:/icon&amp;a&quot;.png"));
        assert!(xml.contains("<audio silent=\"true\"/>"));
        assert_eq!(xml.matches("<action ").count(), 1);
        assert_eq!(xml.matches("landrop-notification://updates").count(), 2);
    }

    #[test]
    fn bounds_unicode_preview_and_removes_invalid_xml_and_bidi_controls() {
        assert_eq!(
            notification_text("  hi\0\u{7}\u{fffe}\u{ffff}\u{202e}!  ", 120),
            "hi!"
        );
        assert_eq!(
            notification_text(&"😊".repeat(500), 320).chars().count(),
            320
        );
    }

    #[test]
    fn activation_queue_is_bounded_and_retains_recent_requests() {
        let state = NotificationState::default();
        for _ in 0..100 {
            state.enqueue(NotificationTarget::Updates {});
        }
        let queue = state.pending.lock().unwrap();
        assert_eq!(queue.len(), MAX_PENDING_ACTIVATIONS);
        assert_ne!(queue[0].id, queue[1].id);
    }
}
