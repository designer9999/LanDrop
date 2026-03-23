mod commands;
mod lan;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Store app handle for LAN events
            let handle = app.handle().clone();
            app.manage(lan::LanState::new(handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::start_lan,
            commands::stop_lan,
            commands::lan_send_text,
            commands::lan_send_files,
            commands::get_file_info,
            commands::show_in_explorer,
            commands::get_thumbnail,
            commands::get_local_ip,
            commands::save_clipboard_image,
            commands::get_clipboard_files,
            commands::read_file_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LanDrop");
}
