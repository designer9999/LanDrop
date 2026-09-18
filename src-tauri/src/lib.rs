mod commands;
mod lan;
mod notifications;
mod path_utils;

use tauri::Manager;

#[cfg(desktop)]
use std::sync::atomic::{AtomicBool, Ordering};

/// When true, the app is actually quitting — don't intercept CloseRequested.
#[cfg(desktop)]
static QUITTING: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[expect(
    clippy::expect_used,
    reason = "startup cannot continue without an app data dir or a running app"
)]
pub fn run() {
    // Linux Wayland: force native Wayland backend for sharp fractional scaling.
    // WebKitGTK's DMA-BUF renderer crashes on NVIDIA + Wayland — disable it.
    //
    // NOTE: AppImage's linuxdeploy-plugin-gtk hook exports GDK_BACKEND=x11 BEFORE
    // this binary runs. We must FORCE-override when on a Wayland session, ignoring
    // whatever the AppRun script set.
    #[cfg(target_os = "linux")]
    {
        if commands::platform::is_wayland_session() {
            // Always set, even if AppImage hook set it to x11
            std::env::set_var("GDK_BACKEND", "wayland");
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    let mut builder = tauri::Builder::default().manage(notifications::NotificationState::default());

    // Single instance — desktop only and must be first plugin. If app is already
    // running, focus the existing window instead of opening a second instance.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            notifications::handle_launch_args(app, args);
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }));
    }

    builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init());

    // Android-only file helper plugin (FileProvider open, content URI name resolution)
    #[cfg(target_os = "android")]
    {
        builder = builder.plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("file-helper")
                .setup(|_app, api| {
                    api.register_android_plugin(
                        "io.github.designer9999.landrop",
                        "FileHelperPlugin",
                    )?;
                    Ok(())
                })
                .build(),
        );
    }

    // Global shortcut plugin — desktop only
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    }

    builder
        .setup(|app| {
            // Updater plugin — desktop only (not available on mobile)
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            // Window state — skip on Wayland (compositors like Hyprland/Sway don't
            // support saving window position, so the plugin just thrashes for nothing)
            #[cfg(desktop)]
            {
                let skip_window_state = commands::platform::is_wayland_session();
                if !skip_window_state {
                    app.handle()
                        .plugin(tauri_plugin_window_state::Builder::default().build())?;
                }
            }

            // Load device identity and create LAN service
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir().expect("app data dir");
            let identity = lan::identity::DeviceIdentity::load_or_create(&data_dir);
            app.manage(lan::LanState::new(handle, identity, data_dir));
            notifications::handle_launch_args(
                app.handle(),
                std::env::args_os()
                    .skip(1)
                    .filter_map(|arg| arg.into_string().ok()),
            );

            #[cfg(target_os = "android")]
            {
                use tauri_plugin_notification::{Channel, Importance, NotificationExt, Visibility};
                let _ = app.notification().create_channel(
                    Channel::builder("landrop-incoming-v2", "LanDrop incoming")
                        .description("Incoming LanDrop files and messages")
                        .importance(Importance::High)
                        .visibility(Visibility::Private)
                        .vibration(true)
                        .build(),
                );
            }

            // ── System Tray (desktop only) ──
            #[cfg(desktop)]
            {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                use tauri::tray::TrayIconBuilder;

                let show = MenuItemBuilder::with_id("show", "Show LanDrop").build(app)?;
                let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
                let menu = MenuBuilder::new(app)
                    .item(&show)
                    .separator()
                    .item(&quit)
                    .build()?;

                // Dedicated full-canvas 64px artwork avoids inheriting launcher
                // padding and provides enough source pixels up to 400% Windows
                // tray scaling. Embed it: runtime paths break in AppImage.
                let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/64x64.png"))?;

                let _tray = TrayIconBuilder::with_id("main-tray")
                    .icon(icon)
                    .tooltip("LanDrop")
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "show" => {
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.show();
                                let _ = win.unminimize();
                                let _ = win.set_focus();
                            }
                        }
                        "quit" => {
                            QUITTING.store(true, Ordering::SeqCst);
                            if let Some(tray) = app.tray_by_id("main-tray") {
                                let _ = tray.set_visible(false);
                            }
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                            if let Some(win) = tray.app_handle().get_webview_window("main") {
                                let _ = win.show();
                                let _ = win.unminimize();
                                let _ = win.set_focus();
                            }
                        }
                    })
                    .build(app)?;

                // Minimize to tray on close (unless actually quitting)
                if let Some(win) = app.get_webview_window("main") {
                    win.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            if QUITTING.load(Ordering::SeqCst) {
                                return;
                            }
                            api.prevent_close();
                            if let Some(w) = _tray.app_handle().get_webview_window("main") {
                                let _ = w.hide();
                            }
                        }
                    });
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::start_lan_service,
            commands::stop_lan_service,
            commands::refresh_lan_discovery,
            commands::lan_send_text,
            commands::lan_send_files,
            commands::set_default_out_folder,
            commands::set_peer_out_folder,
            commands::set_receive_sort_by_date,
            commands::get_receive_folder_settings,
            commands::set_device_alias,
            commands::get_device_identity,
            commands::get_file_info,
            commands::show_in_explorer,
            commands::get_thumbnail,
            commands::get_platform_info,
            commands::save_clipboard_image,
            commands::get_clipboard_files,
            commands::read_file_preview,
            commands::read_file_bytes,
            commands::set_mica,
            commands::get_explorer_selection,
            commands::open_file,
            commands::open_url,
            commands::save_temp_for_send,
            commands::save_history_file,
            commands::delete_history_files,
            commands::cleanup_send_cache,
            notifications::show_native_notification,
            notifications::take_notification_activations,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LanDrop");
}

#[cfg(test)]
mod icon_tests {
    #[test]
    fn tray_artwork_fills_canvas_and_has_transparent_corners() {
        let icon = image::load_from_memory(include_bytes!("../icons/64x64.png"))
            .expect("tray PNG must decode")
            .to_rgba8();
        assert_eq!(icon.dimensions(), (64, 64));
        for (x, y) in [(0, 0), (63, 0), (0, 63), (63, 63)] {
            assert!(icon.get_pixel(x, y)[3] < 16, "corner must be transparent");
        }
        for (x, y) in [(32, 0), (32, 63), (0, 32), (63, 32)] {
            let pixel = icon.get_pixel(x, y);
            assert!(pixel[3] >= 240, "tile must reach every canvas edge");
            assert!(pixel[0] > 200 && pixel[1] < 130 && pixel[2] < 80);
        }
        assert_eq!(
            include_bytes!("../icons/128x128.png").as_slice(),
            include_bytes!("../../public/app-icon.png").as_slice(),
            "in-app and native notification branding must match"
        );
    }

    #[test]
    fn windows_icon_includes_small_and_large_frames() {
        let bytes = include_bytes!("../icons/icon.ico");
        assert_eq!(&bytes[..4], &[0, 0, 1, 0]);
        let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        assert!(bytes.len() >= 6 + count * 16);
        let dimensions: Vec<u16> = (0..count)
            .map(|index| {
                let entry = 6 + index * 16;
                assert_eq!(bytes[entry], bytes[entry + 1], "ICO frame must be square");
                match bytes[entry] {
                    0 => 256,
                    size => u16::from(size),
                }
            })
            .collect();
        for required in [16, 24, 32, 48, 64, 256] {
            assert!(dimensions.contains(&required), "missing {required}px frame");
        }
    }
}
