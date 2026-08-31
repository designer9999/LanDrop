//! Shell integration: reveal in file manager, open files/URLs, and the
//! Windows Explorer selection (COM).

use std::path::PathBuf;

#[cfg(target_os = "linux")]
use super::clipboard::linux_clipboard_files;
#[cfg(target_os = "macos")]
use super::clipboard::macos_clipboard_files;

#[tauri::command]
pub async fn show_in_explorer(path: String) -> Result<bool, String> {
    // Spawns child processes (and on Linux a dbus-send with a 2 s reply
    // timeout) — keep it off the async runtime workers.
    tokio::task::spawn_blocking(move || show_in_explorer_blocking(path))
        .await
        .map_err(|e| e.to_string())?
}

fn show_in_explorer_blocking(path: String) -> Result<bool, String> {
    let requested = PathBuf::from(path);
    let target = if requested.exists() {
        requested
    } else if let Some(parent) = requested.parent().filter(|parent| parent.exists()) {
        parent.to_path_buf()
    } else {
        return Err("Path does not exist".to_string());
    };

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Strip UNC prefix (\\?\) from canonicalized paths — Explorer can't handle them
        let path_str = target
            .canonicalize()
            .ok()
            .map(|p| {
                let s = p.to_string_lossy().to_string();
                s.strip_prefix(r"\\?\").map(|s| s.to_string()).unwrap_or(s)
            })
            .unwrap_or_else(|| target.to_string_lossy().to_string());

        if target.is_file() {
            // /select, must be one comma-separated argument
            std::process::Command::new("explorer")
                .raw_arg(format!("/select,\"{}\"", path_str))
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            std::process::Command::new("explorer")
                .arg(&path_str)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let path_str = target.to_string_lossy().to_string();
        if target.is_file() {
            // -R reveals (selects) the file in Finder
            let _ = std::process::Command::new("open")
                .args(["-R", &path_str])
                .spawn();
        } else {
            let _ = std::process::Command::new("open").arg(&path_str).spawn();
        }
    }

    #[cfg(target_os = "linux")]
    {
        let path_str = target.to_string_lossy().to_string();
        let parent = target
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone());

        let to_uri = |path: &str| -> String {
            let encoded = path
                .split('/')
                .map(|seg| urlencoding::encode(seg).into_owned())
                .collect::<Vec<_>>()
                .join("/");
            format!("file://{}", encoded)
        };

        // Build a clean Command — strip AppImage's LD_LIBRARY_PATH and
        // similar vars that break native file managers when spawned from
        // inside an AppImage (loads incompatible bundled libs).
        let clean_cmd = |bin: &str| -> std::process::Command {
            let mut c = std::process::Command::new(bin);
            for var in [
                "LD_LIBRARY_PATH",
                "LD_PRELOAD",
                "GST_PLUGIN_PATH",
                "GST_PLUGIN_SYSTEM_PATH",
                "GST_PLUGIN_SCANNER",
                "GTK_PATH",
                "GIO_MODULE_DIR",
                "GIO_EXTRA_MODULES",
                "GDK_PIXBUF_MODULE_FILE",
                "GDK_PIXBUF_MODULEDIR",
                "QT_PLUGIN_PATH",
                "PYTHONHOME",
                "PYTHONPATH",
            ] {
                // Restore from original if AppRun saved it; else just remove it
                if let Ok(orig) = std::env::var(format!("APPIMAGE_ORIG_{}", var)) {
                    c.env(var, orig);
                } else {
                    c.env_remove(var);
                }
            }
            c
        };

        let try_spawn =
            |bin: &str, args: &[&str]| -> bool { clean_cmd(bin).args(args).spawn().is_ok() };

        let mut tried: Vec<String> = Vec::new();
        let mut record = |bin: &str, ok: bool| {
            tried.push(format!("{}={}", bin, if ok { "ok" } else { "fail" }));
            ok
        };

        if target.is_file() {
            let file_uri = to_uri(&path_str);
            let dbus_arg = format!("array:string:{}", file_uri);

            // 1. DBus FileManager1 ShowItems — wait for reply with --print-reply
            //    so we know if a file manager actually responded
            let dbus_ok = clean_cmd("dbus-send")
                .args([
                    "--session",
                    "--print-reply",
                    "--reply-timeout=2000",
                    "--dest=org.freedesktop.FileManager1",
                    "/org/freedesktop/FileManager1",
                    "org.freedesktop.FileManager1.ShowItems",
                    &dbus_arg,
                    "string:",
                ])
                .output()
                .is_ok_and(|o| o.status.success());

            if record("dbus-FileManager1", dbus_ok) {
                return Ok(true);
            }

            let opened = record("nautilus", try_spawn("nautilus", &[&file_uri]))
                || record("dolphin", try_spawn("dolphin", &["--select", &path_str]))
                || record("nemo", try_spawn("nemo", &[&path_str]))
                || record("thunar", try_spawn("thunar", &[&parent]))
                || record("pcmanfm", try_spawn("pcmanfm", &[&parent]))
                || record("gio", try_spawn("gio", &["open", &parent]))
                || record("xdg-open", try_spawn("xdg-open", &[&parent]));

            if !opened {
                return Err(format!(
                    "No file manager succeeded. Tried: [{}]. \
                     Install nautilus, dolphin, nemo, thunar, pcmanfm, glib2 (gio), or xdg-utils.",
                    tried.join(", ")
                ));
            }
        } else if !try_spawn("xdg-open", &[&path_str]) && !try_spawn("gio", &["open", &path_str]) {
            return Err("xdg-open or gio not found. Install xdg-utils or glib2.".to_string());
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = target;
    }

    Ok(true)
}

/// Get currently selected files from the active Windows Explorer window via native COM.
/// Uses IShellWindows → IWebBrowser2 → IShellBrowser → IFolderView2 → IShellItemArray.
#[tauri::command]
pub async fn get_explorer_selection() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        // COM shell objects require STA — run on a dedicated thread
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = explorer_selection_com();
            let _ = tx.send(result);
        });
        rx.await.map_err(|_| "Thread join failed".to_string())?
    }
    #[cfg(target_os = "linux")]
    {
        // No "active file manager selection" concept on Linux — fall back to
        // clipboard files (user copies in their file manager, then triggers send)
        tokio::task::spawn_blocking(linux_clipboard_files)
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(macos_clipboard_files)
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Ok(vec![])
    }
}

#[cfg(target_os = "windows")]
#[expect(unsafe_code, reason = "COM shell interop requires unsafe FFI")]
fn explorer_selection_com() -> Result<Vec<String>, String> {
    use windows::core::Interface as _;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IServiceProvider, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        IFolderView2, IShellBrowser, IShellItem, IShellItemArray, IShellView, IShellWindows,
        IWebBrowser2, SID_STopLevelBrowser, ShellWindows, SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let result = (|| -> Result<Vec<String>, String> {
            let shell_windows: IShellWindows =
                CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).map_err(|e| format!("{e}"))?;

            let fg_hwnd = GetForegroundWindow();
            let count = shell_windows.Count().map_err(|e| format!("{e}"))?;

            for i in 0..count {
                // Build VARIANT with VT_I4 for the index
                let v = windows::Win32::System::Variant::VARIANT::from(i);

                let Ok(disp) = shell_windows.Item(&v) else {
                    continue;
                };
                let Ok(wb) = disp.cast::<IWebBrowser2>() else {
                    continue;
                };
                let Ok(hwnd_val) = wb.HWND() else { continue };

                let wnd = HWND(hwnd_val.0 as *mut _);
                if wnd != fg_hwnd {
                    continue;
                }

                let sp: IServiceProvider = wb.cast().map_err(|e| format!("{e}"))?;
                let sb: IShellBrowser = sp
                    .QueryService(&SID_STopLevelBrowser)
                    .map_err(|e| format!("{e}"))?;
                let sv: IShellView = sb.QueryActiveShellView().map_err(|e| format!("{e}"))?;
                let fv: IFolderView2 = sv.cast().map_err(|e| format!("{e}"))?;
                let selection: IShellItemArray =
                    fv.GetSelection(false).map_err(|e| format!("{e}"))?;
                let n = selection.GetCount().map_err(|e| format!("{e}"))?;

                let mut paths = Vec::with_capacity(n as usize);
                for j in 0..n {
                    let item: IShellItem = match selection.GetItemAt(j) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let name_ptr: windows::core::PWSTR =
                        match item.GetDisplayName(SIGDN_FILESYSPATH) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                    let result = name_ptr.to_string();
                    // Free COM-allocated PWSTR to prevent memory leak
                    windows::Win32::System::Com::CoTaskMemFree(Some(name_ptr.0 as *const _));
                    if let Ok(s) = result {
                        if !s.is_empty() {
                            paths.push(s);
                        }
                    }
                }
                return Ok(paths);
            }
            Ok(vec![])
        })();

        CoUninitialize();
        result
    }
}

/// Open a file with the system's default handler
#[tauri::command]
pub async fn open_file(path: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let target = PathBuf::from(&path);

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if !target.exists() {
            return Err("open_file: path does not exist".to_string());
        }
        if !target.is_dir() {
            return Err("open_file: desktop open is restricted to folders".to_string());
        }
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = target;
    }

    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("open_file: {e}"))
}

#[tauri::command]
pub async fn open_url(url: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let parsed = tauri::Url::parse(&url).map_err(|_| "open_url: invalid URL".to_string())?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("github.com") {
        return Err("open_url: only https://github.com links are allowed".to_string());
    }

    app.opener()
        .open_url(parsed, None::<&str>)
        .map_err(|e| format!("open_url: {e}"))
}
