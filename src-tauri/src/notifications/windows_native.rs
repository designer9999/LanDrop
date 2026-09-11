//! Inbox WinRT compatibility path for the existing NSIS/Win32 distribution.
//! Unlike callback-only wrappers, protocol activation also works after exit.
//! The NSIS hook supplies the matching AUMID shortcut, stub CLSID and protocol.

use super::{toast_xml, NativeNotification, NotificationTarget};
use tauri::{AppHandle, Manager};
use windows::{
    core::{Interface, HSTRING},
    Data::Xml::Dom::XmlDocument,
    Foundation::{DateTime, IReference, PropertyValue},
    UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
};

pub(super) fn show(app: &AppHandle, notification: &NativeNotification) -> Result<(), String> {
    let icon_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&icon_dir).map_err(|error| error.to_string())?;
    // Never load an image supplied by a peer or webview. A fixed bundled asset
    // avoids arbitrary file reads and network requests from the Windows shell.
    let icon_path = icon_dir.join("notification-icon.png");
    let icon = include_bytes!("../../icons/128x128.png");
    if std::fs::read(&icon_path).ok().as_deref() != Some(icon.as_slice()) {
        std::fs::write(&icon_path, icon).map_err(|error| error.to_string())?;
    }
    let icon_uri = tauri::Url::from_file_path(icon_path)
        .map_err(|_| "Notification icon path is invalid".to_owned())?;
    show_windows(notification, icon_uri.as_str(), &app.config().identifier)
        .map_err(|error| format!("Windows notification failed: {error}"))
}

fn show_windows(
    notification: &NativeNotification,
    icon_uri: &str,
    app_id: &str,
) -> windows::core::Result<()> {
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(toast_xml(notification, icon_uri)))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    // Replace older notifications for a conversation, rather than flooding
    // Notification Center during a burst. Updates have their own stable slot.
    let tag = match &notification.target {
        NotificationTarget::Peer { peer_id } => peer_id.replace('-', ""),
        NotificationTarget::Updates {} => "updates".to_owned(),
    };
    toast.SetTag(&HSTRING::from(tag))?;
    toast.SetGroup(&HSTRING::from("landrop"))?;
    // Windows DateTime uses 100-nanosecond ticks since 1601-01-01 UTC.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expiry = DateTime {
        UniversalTime: ((now + 11_644_473_600 + 86_400) * 10_000_000) as i64,
    };
    let expiry = PropertyValue::CreateDateTime(expiry)?.cast::<IReference<DateTime>>()?;
    toast.SetExpirationTime(&expiry)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?;
    if notifier.Setting()? != NotificationSetting::Enabled {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80070005u32 as i32),
            "Notifications are disabled in Windows Settings or by policy",
        ));
    }
    notifier.Show(&toast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_toast_is_valid_windows_xml_and_text_stays_literal() -> windows::core::Result<()> {
        let notification = NativeNotification {
            title: "Peer <&> name".to_owned(),
            body: "<action arguments=\"file:///secret\"/>".to_owned(),
            target: NotificationTarget::Updates {},
            silent: false,
        };
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(toast_xml(
            &notification,
            "file:///C:/icon.png",
        )))?;
        let text = document.GetElementsByTagName(&HSTRING::from("text"))?;
        assert_eq!(text.Item(0)?.InnerText()?.to_string(), notification.title);
        assert_eq!(text.Item(1)?.InnerText()?.to_string(), notification.body);
        assert_eq!(
            document
                .GetElementsByTagName(&HSTRING::from("action"))?
                .Length()?,
            1
        );
        // Construct but do not show a toast: this checks the actual Windows XML
        // API without altering OS notification settings, history or registry.
        let _ = ToastNotification::CreateToastNotification(&document)?;
        for target in [
            NotificationTarget::Updates {},
            NotificationTarget::Peer {
                peer_id: "02b54e69-ec9a-422f-b5a3-5f906348b439".to_owned(),
            },
        ] {
            let uri = target.uri();
            let normalized =
                windows::Foundation::Uri::CreateUri(&HSTRING::from(&uri))?.AbsoluteUri()?;
            assert_eq!(normalized.to_string(), uri);
            assert_eq!(
                NotificationTarget::from_uri(&normalized.to_string()),
                Some(target)
            );
        }
        Ok(())
    }
}
