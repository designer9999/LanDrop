//! Tauri command surface, grouped by domain.
//!
//! This module keeps the LAN service commands; filesystem info, clipboard,
//! shell integration, storage, and platform helpers live in their own files.

pub(crate) mod clipboard;
pub(crate) mod fs_info;
pub(crate) mod platform;
pub(crate) mod shell;
pub(crate) mod storage;

pub use clipboard::*;
pub use fs_info::*;
pub use platform::*;
pub use shell::*;
pub use storage::*;

use crate::lan::LanState;
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

#[derive(Serialize)]
pub struct ReceiveFolderSettingsResponse {
    pub default_out_folder: String,
    pub peer_folders: HashMap<String, String>,
    pub sort_by_date: bool,
}

#[tauri::command]
pub async fn start_lan_service(state: State<'_, LanState>) -> Result<(), String> {
    state.service.start().await
}

#[tauri::command]
pub async fn stop_lan_service(state: State<'_, LanState>) -> Result<(), String> {
    state.service.stop().await;
    Ok(())
}

/// Force a fresh mDNS scan by restarting the discovery service.
/// Useful when peers can't find each other due to lost multicast packets.
#[tauri::command]
pub async fn refresh_lan_discovery(state: State<'_, LanState>) -> Result<(), String> {
    state.service.stop().await;
    // start() awaits the previous run's JoinHandle, so the old daemon has sent
    // its mDNS goodbye and released the TCP port before the new one spawns.
    state.service.start().await
}

#[tauri::command]
pub async fn lan_send_text(
    peer_id: String,
    text: String,
    peer_ip: Option<String>,
    state: State<'_, LanState>,
) -> Result<bool, String> {
    state
        .service
        .send_text(&peer_id, peer_ip.as_deref(), &text)
        .await
}

#[tauri::command]
pub async fn lan_send_files(
    peer_id: String,
    paths: Vec<String>,
    peer_ip: Option<String>,
    state: State<'_, LanState>,
) -> Result<bool, String> {
    state
        .service
        .send_files(&peer_id, peer_ip.as_deref(), &paths)
        .await
}

#[tauri::command]
pub async fn set_default_out_folder(
    folder: String,
    state: State<'_, LanState>,
) -> Result<(), String> {
    state.service.set_default_folder(&folder).await;
    Ok(())
}

#[tauri::command]
pub async fn set_peer_out_folder(
    peer_id: String,
    folder: String,
    state: State<'_, LanState>,
) -> Result<(), String> {
    state.service.set_peer_folder(&peer_id, &folder).await;
    Ok(())
}

#[tauri::command]
pub async fn get_receive_folder_settings(
    state: State<'_, LanState>,
) -> Result<ReceiveFolderSettingsResponse, String> {
    let (default_out_folder, peer_folders, sort_by_date) =
        state.service.get_folder_settings().await;
    Ok(ReceiveFolderSettingsResponse {
        default_out_folder,
        peer_folders,
        sort_by_date,
    })
}

#[tauri::command]
pub async fn set_receive_sort_by_date(
    enabled: bool,
    state: State<'_, LanState>,
) -> Result<(), String> {
    state.service.set_sort_by_date(enabled).await;
    Ok(())
}

#[tauri::command]
pub async fn set_device_alias(alias: String, state: State<'_, LanState>) -> Result<String, String> {
    Ok(state.service.set_alias(&alias).await)
}

#[tauri::command]
pub async fn get_device_identity(state: State<'_, LanState>) -> Result<serde_json::Value, String> {
    let id = state.service.get_identity().await;
    Ok(serde_json::json!({
        "id": id.id.to_string(),
        "alias": id.alias,
        "device_type": id.device_type,
    }))
}
