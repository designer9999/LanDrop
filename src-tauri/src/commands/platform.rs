//! Platform/status queries and window effects.

use serde::Serialize;

/// True when running inside a Wayland session (always false off Linux).
pub(crate) fn is_wayland_session() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland")
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub ok: bool,
    pub app_version: String,
    pub local_ip: String,
}

#[tauri::command]
pub async fn get_status() -> Result<StatusResponse, String> {
    let local_ip = get_local_ip_inner();
    Ok(StatusResponse {
        ok: true,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        local_ip,
    })
}

/// Returns platform info so the UI can disable features that don't work
/// (e.g. global hotkeys are blocked by Wayland's security model)
#[tauri::command]
pub async fn get_platform_info() -> Result<serde_json::Value, String> {
    let is_wayland = is_wayland_session();
    Ok(serde_json::json!({
        "os": std::env::consts::OS,
        "is_wayland": is_wayland,
        "supports_global_hotkeys": !is_wayland,
        "supports_window_state_persistence": !is_wayland,
    }))
}

fn get_local_ip_inner() -> String {
    crate::lan::discovery::get_local_ipv4()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[tauri::command]
pub async fn set_mica(handle: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        let window = handle.get_webview_window("main").ok_or("No main window")?;
        if enabled {
            // Minimal tint — let CSS control the darkness
            let tint: window_vibrancy::Color = (0, 0, 0, 1);
            if window_vibrancy::apply_acrylic(&window, Some(tint)).is_err() {
                let _ = window_vibrancy::apply_mica(&window, Some(true));
            }
        } else {
            let _ = window_vibrancy::clear_acrylic(&window);
            let _ = window_vibrancy::clear_mica(&window);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (handle, enabled);
    }
    Ok(())
}
