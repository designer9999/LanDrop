"""
Croc Transfer — Python backend API.
Exposed to the Svelte frontend via window.pywebview.api.

Requires Python 3.14+.
"""

import base64
import glob as _glob
import json
import logging
import mimetypes
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import webbrowser

import webview
from webview.dom import DOMEventHandler

from backend.lan_server import LANPeer
from backend.updater import check_for_updates, download_update
from backend.version import APP_VERSION

logger = logging.getLogger(__name__)

_NO_WINDOW = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0


def _file_size_str(size_bytes: int) -> str:
    """Human-readable file size."""
    for unit in ("B", "KB", "MB", "GB"):
        if size_bytes < 1024:
            return f"{size_bytes:.1f} {unit}" if unit != "B" else f"{size_bytes} B"
        size_bytes /= 1024
    return f"{size_bytes:.1f} TB"


def _safe_close_process(proc: subprocess.Popen | None) -> None:
    """Safely close a subprocess and its pipes."""
    if proc is None:
        return
    try:
        if proc.stdout:
            proc.stdout.close()
        if proc.stderr:
            proc.stderr.close()
    except Exception:
        pass
    try:
        proc.terminate()
        proc.wait(timeout=5)
    except Exception:
        try:
            proc.kill()
            proc.wait(timeout=3)
        except Exception:
            pass


class CrocAPI:
    """pywebview js_api — each public method becomes window.pywebview.api.<name>()

    IMPORTANT: All non-API attributes MUST start with _ to prevent pywebview
    from recursively enumerating them (WinForms AccessibilityObject issue).
    """

    def __init__(self) -> None:
        self._window: webview.Window | None = None
        self._lock = threading.Lock()
        self._process: subprocess.Popen | None = None
        self._croc_path = self._find_croc()
        self._lan_peer: LANPeer | None = None
        self._lan_peer_lock = threading.Lock()
        self._window_focused = True
        self._notifications_enabled = True

    def set_window(self, window: webview.Window) -> None:
        self._window = window
        # Drop handler must bind AFTER window is loaded (DOM not ready before)
        window.events.loaded += lambda: self._bind_drop_handler(window)

    def _bind_drop_handler(self, window: webview.Window) -> None:
        """Bind native drop handler to get full file paths from pywebview."""
        def _on_drag(e):
            pass

        def _on_drop(e):
            files = e.get("dataTransfer", {}).get("files", [])
            paths = []
            for f in files:
                full_path = f.get("pywebviewFullPath")
                if full_path and os.path.exists(full_path):
                    paths.append(full_path)
            if paths:
                self._js_event("files_dropped", {"paths": paths})

        try:
            window.dom.document.events.dragover += DOMEventHandler(_on_drag, True, False, debounce=500)
            window.dom.document.events.drop += DOMEventHandler(_on_drop, True, False)
        except Exception as e:
            logger.warning("Failed to bind drop handler: %s", e)

    def set_focused(self, focused: bool) -> None:
        self._window_focused = focused

    def set_notifications(self, enabled: bool) -> None:
        self._notifications_enabled = enabled

    def _notify(self, title: str) -> None:
        """Play system notification sound when app is not focused."""
        if self._window_focused or not self._notifications_enabled:
            return
        if sys.platform != "win32":
            return
        try:
            import winsound

            winsound.MessageBeep(winsound.MB_OK)
        except Exception:
            pass

    def _cleanup(self) -> None:
        """Kill croc processes and stop LAN peer on exit."""
        _safe_close_process(self._process)
        self._process = None
        self._stop_lan_internal()

    # ── Croc discovery ──

    @staticmethod
    def _find_croc() -> str | None:
        if found := shutil.which("croc"):
            return found

        local_app = os.environ.get("LOCALAPPDATA", "")
        user_profile = os.environ.get("USERPROFILE", "")
        candidates: list[str] = []

        if local_app:
            candidates.append(
                os.path.join(
                    local_app,
                    "Microsoft",
                    "WinGet",
                    "Packages",
                    "schollz.croc_Microsoft.Winget.Source_8wekyb3d8bbwe",
                    "croc.exe",
                )
            )
        if user_profile:
            candidates.append(
                os.path.join(
                    user_profile,
                    "AppData",
                    "Local",
                    "Microsoft",
                    "WinGet",
                    "Packages",
                    "schollz.croc_Microsoft.Winget.Source_8wekyb3d8bbwe",
                    "croc.exe",
                )
            )

        if local_app:
            candidates.append(
                os.path.join(local_app, "Microsoft", "WinGet", "Links", "croc.exe")
            )
        if user_profile:
            candidates.append(
                os.path.join(
                    user_profile,
                    "AppData",
                    "Local",
                    "Microsoft",
                    "WinGet",
                    "Links",
                    "croc.exe",
                )
            )

        if user_profile:
            candidates.append(os.path.join(user_profile, "scoop", "shims", "croc.exe"))

        candidates.extend(
            [
                r"C:\ProgramData\chocolatey\bin\croc.exe",
                r"C:\Program Files\croc\croc.exe",
                r"C:\Program Files (x86)\croc\croc.exe",
                os.path.expanduser("~/.local/bin/croc"),
                "/usr/local/bin/croc",
            ]
        )

        for p in candidates:
            if p and os.path.isfile(p):
                logger.info("Found croc at: %s", p)
                return p

        for base in [
            local_app,
            os.path.join(user_profile, "AppData", "Local") if user_profile else "",
        ]:
            if not base:
                continue
            pattern = os.path.join(
                base, "Microsoft", "WinGet", "Packages", "schollz.croc*", "croc.exe"
            )
            if matches := _glob.glob(pattern):
                logger.info("Found croc via glob: %s", matches[0])
                return matches[0]

        logger.warning(
            "croc not found. Searched PATH and %d candidates. "
            "LOCALAPPDATA=%r, USERPROFILE=%r",
            len(candidates),
            local_app,
            user_profile,
        )
        return None

    # ── Public API (called from JS) ──

    @staticmethod
    def _get_local_ip() -> str:
        """Get the LAN IP address of this machine."""
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
                s.connect(("8.8.8.8", 80))
                return s.getsockname()[0]
        except Exception:
            return "unknown"

    def get_status(self) -> dict:
        local_ip = self._get_local_ip()
        if not self._croc_path:
            return {
                "ok": False,
                "error": "croc not found. Install: winget install schollz.croc",
                "app_version": APP_VERSION,
                "local_ip": local_ip,
            }
        try:
            r = subprocess.run(
                [self._croc_path, "--version"],
                capture_output=True,
                text=True,
                timeout=5,
                creationflags=_NO_WINDOW,
            )
            return {
                "ok": True,
                "version": r.stdout.strip(),
                "app_version": APP_VERSION,
                "local_ip": local_ip,
            }
        except (subprocess.TimeoutExpired, FileNotFoundError, OSError) as e:
            return {
                "ok": False,
                "error": str(e),
                "app_version": APP_VERSION,
                "local_ip": local_ip,
            }

    def check_update(self) -> dict:
        """Check GitHub for a newer app version."""
        return check_for_updates()

    def download_update(self, url: str) -> None:
        """Download update .exe in background."""

        def _run():
            def on_progress(pct):
                self._js_event("update_progress", {"percent": pct})

            try:
                path = download_update(url, progress_callback=on_progress)
                if path:
                    self._js_event(
                        "update_ready",
                        {
                            "path": path,
                            "file_name": os.path.basename(path),
                        },
                    )
                else:
                    self._js_log(
                        "error", "Update download failed — file too small or corrupt"
                    )
                    self._js_event("update_failed", {})
            except Exception as e:
                logger.error("Download update error: %s", e)
                self._js_log("error", f"Download failed: {e}")
                self._js_event("update_failed", {})

        threading.Thread(target=_run, name="update-download", daemon=True).start()

    def launch_update(self, path: str) -> None:
        """Replace the current exe with the downloaded update and restart."""
        if not os.path.isfile(path):
            return

        new_size = os.path.getsize(path)
        current_exe = sys.executable
        log_path = os.path.join(tempfile.gettempdir(), "crude_update_log.txt")

        if getattr(sys, "frozen", False) and current_exe.endswith(".exe"):
            ps1 = os.path.join(tempfile.gettempdir(), "crude_update.ps1")
            script = f"""
$src = '{path.replace("'", "''")}'
$dst = '{current_exe.replace("'", "''")}'
$expectedSize = {new_size}
$log = '{log_path.replace("'", "''")}'

function Log($msg) {{
    "$(Get-Date -Format 'HH:mm:ss') $msg" | Out-File $log -Append -Encoding utf8
}}

Log "Update script started"
Log "Source: $src ($((Get-Item $src).Length) bytes)"
Log "Target: $dst"

for ($w = 0; $w -lt 30; $w++) {{
    try {{
        $fs = [System.IO.File]::Open($dst, 'Open', 'ReadWrite', 'None')
        $fs.Close()
        Log "File unlocked after $w seconds"
        break
    }} catch {{
        if ($w -eq 0) {{ Log "Waiting for file to unlock..." }}
        Start-Sleep -Seconds 1
    }}
}}

$copied = $false
for ($i = 0; $i -lt 10; $i++) {{
    try {{
        Copy-Item -Path $src -Destination $dst -Force
        $copiedSize = (Get-Item $dst).Length
        Log "Copy attempt $($i+1): copied $copiedSize bytes (expected $expectedSize)"
        if ($copiedSize -eq $expectedSize) {{
            $copied = $true
            Log "Copy verified OK"
            break
        }}
    }} catch {{
        Log "Copy attempt $($i+1) failed: $_"
    }}
    Start-Sleep -Seconds 2
}}

if ($copied) {{
    Log "Launching updated exe: $dst"
    Start-Process -FilePath $dst
}} else {{
    Log "All copy attempts failed. Launching from temp: $src"
    Start-Process -FilePath $src
}}

Start-Sleep -Seconds 3
Remove-Item -Path $src -Force -ErrorAction SilentlyContinue
Remove-Item -Path $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue
"""
            with open(ps1, "w", encoding="utf-8") as f:
                f.write(script)

            subprocess.Popen(
                [
                    "powershell.exe",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-WindowStyle",
                    "Hidden",
                    "-File",
                    ps1,
                ],
                creationflags=_NO_WINDOW,
            )
        else:
            os.startfile(path)

        self._stop_lan_internal()

        if self._window:
            self._window.destroy()
        sys.exit(0)

    def install_croc(self) -> None:
        """Install croc via winget in background."""

        def _run():
            proc = None
            try:
                self._js_event("install_croc_start", {})
                self._js_log("info", "Installing croc via winget...")
                proc = subprocess.Popen(
                    [
                        "winget",
                        "install",
                        "--id",
                        "schollz.croc",
                        "-e",
                        "--accept-source-agreements",
                        "--accept-package-agreements",
                    ],
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    bufsize=1,
                    creationflags=_NO_WINDOW,
                )
                for line in proc.stdout:
                    if line := line.rstrip():
                        self._js_log("info", line)
                proc.wait(timeout=300)
                if proc.returncode == 0:
                    self._croc_path = self._find_croc()
                    if self._croc_path:
                        self._js_log("success", "croc installed successfully!")
                        self._js_event("install_croc_done", {"success": True})
                    else:
                        self._js_log(
                            "warn",
                            "croc installed but not found in PATH. Restart the app.",
                        )
                        self._js_event(
                            "install_croc_done",
                            {"success": True, "needs_restart": True},
                        )
                else:
                    self._js_log("error", "croc installation failed")
                    self._js_event("install_croc_done", {"success": False})
            except FileNotFoundError:
                self._js_log(
                    "error",
                    "winget not found. Install croc manually: "
                    "https://github.com/schollz/croc#install",
                )
                self._js_event(
                    "install_croc_done", {"success": False, "no_winget": True}
                )
            except (subprocess.TimeoutExpired, OSError) as e:
                self._js_log("error", f"Installation failed: {e}")
                self._js_event("install_croc_done", {"success": False})
            finally:
                _safe_close_process(proc)

        threading.Thread(target=_run, name="croc-install", daemon=True).start()

    def open_url(self, url: str) -> None:
        """Open a URL in the default browser."""
        webbrowser.open(url)

    def open_folder(self, path: str) -> bool:
        """Open a folder in the system file explorer."""
        path = os.path.normpath(path)
        if os.path.isfile(path):
            path = os.path.dirname(path)
        if not os.path.isdir(path):
            return False
        match sys.platform:
            case "win32":
                os.startfile(path)
            case "darwin":
                subprocess.Popen(["open", path])
            case _:
                subprocess.Popen(["xdg-open", path])
        return True

    def copy_to_clipboard(self, text: str) -> None:
        """Copy text to system clipboard."""
        if self._window:
            self._window.evaluate_js(
                f"navigator.clipboard.writeText({json.dumps(text)})"
            )

    def pick_files(self) -> list[str] | None:
        if not self._window:
            return None
        result = self._window.create_file_dialog(
            webview.FileDialog.OPEN,
            allow_multiple=True,
            file_types=("All files (*.*)",),
        )
        return [str(p) for p in result] if result else None

    def pick_folder(self) -> list[str] | None:
        if not self._window:
            return None
        result = self._window.create_file_dialog(webview.FileDialog.FOLDER)
        return [str(p) for p in result] if result else None

    def pick_save_folder(self) -> str | None:
        """Pick a folder to save received files into."""
        if not self._window:
            return None
        result = self._window.create_file_dialog(webview.FileDialog.FOLDER)
        if result and len(result) > 0:
            return str(result[0])
        return None

    def get_file_info(self, path: str) -> dict | None:
        """Return file/folder name, size, type for display."""
        path = os.path.normpath(path)
        if not os.path.exists(path):
            return None
        name = os.path.basename(path) or path
        if os.path.isdir(path):
            total = 0
            count = 0
            for root, _dirs, files in os.walk(path):
                for f in files:
                    try:
                        total += os.path.getsize(os.path.join(root, f))
                    except OSError:
                        pass
                    count += 1
            return {
                "name": name,
                "size": _file_size_str(total),
                "type": "folder",
                "count": count,
            }
        size = os.path.getsize(path)
        ext = os.path.splitext(name)[1].lower()
        return {"name": name, "size": _file_size_str(size), "type": ext or "file"}

    def _is_lan_connected(self) -> bool:
        """Check if LAN direct peer is connected."""
        with self._lan_peer_lock:
            return self._lan_peer is not None and self._lan_peer._connected

    def send_files(self, paths: list, options: dict | None = None) -> None:
        """Send one or more files/folders with optional croc flags."""
        if self._is_lan_connected():
            self._js_log("info", "LAN direct active — use LAN send instead")
            return
        if not self._croc_path:
            self._js_log("error", "croc not found!")
            return
        valid = [p for p in paths if os.path.exists(p)]
        if not valid:
            self._js_log("error", "No valid files selected")
            return

        self._kill_existing_process()
        names = [os.path.basename(p) for p in valid]
        self._js_event("transfer_start", {"mode": "send", "files": names})
        threading.Thread(
            target=self._run_send,
            args=(valid, options or {}),
            name="croc-send",
            daemon=True,
        ).start()

    def send_text(self, text: str, options: dict | None = None) -> None:
        """Send text content via croc."""
        if self._is_lan_connected():
            self._js_log("info", "LAN direct active — use LAN send instead")
            return
        if not self._croc_path:
            self._js_log("error", "croc not found!")
            return
        text = (text or "").strip()
        if not text:
            self._js_log("error", "No text provided")
            return
        self._kill_existing_process()
        self._js_event("transfer_start", {"mode": "send", "files": ["(text)"]})
        threading.Thread(
            target=self._run_send_text,
            args=(text, options or {}),
            name="croc-send-text",
            daemon=True,
        ).start()

    def receive_file(self, code: str, options: dict | None = None) -> None:
        if self._is_lan_connected():
            self._js_log("info", "LAN direct active — files arrive automatically")
            return
        if not self._croc_path:
            self._js_log("error", "croc not found!")
            return
        code = (code or "").strip()
        if not code:
            self._js_log("error", "No code provided")
            return

        self._kill_existing_process()
        self._js_event("transfer_start", {"mode": "receive", "code": code})
        threading.Thread(
            target=self._run_receive,
            args=(code, options or {}),
            name="croc-receive",
            daemon=True,
        ).start()

    def stop_transfer(self) -> bool:
        with self._lock:
            proc = self._process
            if proc:
                _safe_close_process(proc)
                self._process = None

                self._js_log("warn", "Transfer stopped")
                self._js_event("transfer_done", {"success": False, "stopped": True})
                return True
        return False

    def _kill_existing_process(self) -> None:
        """Kill any running croc process before starting a new one."""
        with self._lock:
            proc = self._process
            if proc:
                _safe_close_process(proc)
                self._process = None

    # ── LAN Direct Transfer ──

    def start_lan(self, code: str, out_folder: str = "") -> None:
        """Start LAN direct transfer for a contact code."""
        code = (code or "").strip()
        if not code:
            return

        # Skip if already running with the same code
        with self._lan_peer_lock:
            if self._lan_peer and self._lan_peer._code == code:
                return

        self._stop_lan_internal()

        def _lan_event(ev: str, d: dict) -> None:
            self._js_event(ev, d)
            if ev in ("lan_text_received", "lan_files_received"):
                self._notify(ev)

        peer = LANPeer(
            code,
            on_event=_lan_event,
            on_log=lambda lv, m: self._js_log(lv, m),
        )
        with self._lan_peer_lock:
            self._lan_peer = peer
        peer.start(out_folder=out_folder)

    def stop_lan(self) -> None:
        """Stop LAN direct transfer."""
        self._stop_lan_internal()

    def _stop_lan_internal(self) -> None:
        with self._lan_peer_lock:
            peer = self._lan_peer
            self._lan_peer = None
        if peer:
            peer.stop()

    def lan_send_text(self, text: str) -> bool:
        """Send text instantly over LAN."""
        with self._lan_peer_lock:
            peer = self._lan_peer
        if peer and peer.connected:
            return peer.send_text(text)
        return False

    def lan_send_files(self, paths: list, out_folder: str = "") -> bool:
        """Send files instantly over LAN."""
        with self._lan_peer_lock:
            peer = self._lan_peer
        if peer and peer.connected:
            display_names = [os.path.basename(p) for p in paths]
            logger.info("LAN send_files: %s", display_names)
            self._js_log("info", f"LAN direct: sending {', '.join(display_names)}")
            success = peer.send_files(paths)
            if success:
                self._js_log("success", f"LAN direct: sent {', '.join(display_names)}")
                self._js_event(
                    "transfer_done",
                    {
                        "success": True,
                        "files": display_names,
                        "mode": "send",
                    },
                )
            else:
                self._js_log("error", f"LAN direct: file send failed (connected={peer.connected})")
            return success
        return False

    def lan_status(self) -> dict:
        """Get LAN direct transfer status."""
        with self._lan_peer_lock:
            peer = self._lan_peer
        if peer:
            return {
                "active": True,
                "connected": peer.connected,
                "peer_ip": peer.peer_ip,
            }
        return {"active": False, "connected": False, "peer_ip": None}

    # ── Internal: build CLI args ──

    def _build_global_args(self, opts: dict) -> list[str]:
        args = [self._croc_path, "--yes", "--ignore-stdin"]

        if opts.get("noCompress"):
            args.append("--no-compress")
        if opts.get("overwrite"):
            args.append("--overwrite")
        if opts.get("curve") and opts["curve"] != "p256":
            args += ["--curve", opts["curve"]]
        if opts.get("relay"):
            args += ["--relay", opts["relay"]]
        if opts.get("relayPass"):
            args += ["--pass", opts["relayPass"]]
        if opts.get("throttleUpload"):
            args += ["--throttleUpload", opts["throttleUpload"]]
        if opts.get("local"):
            args.append("--local")

        return args

    def _build_send_args(self, opts: dict) -> list[str]:
        args = ["send"]

        if opts.get("code"):
            args += ["--code", opts["code"]]
        if opts.get("hash") and opts["hash"] != "xxhash":
            args += ["--hash", opts["hash"]]
        if opts.get("zip"):
            args.append("--zip")
        if opts.get("git"):
            args.append("--git")
        if opts.get("noLocal"):
            args.append("--no-local")
        if opts.get("noMulti"):
            args.append("--no-multi")
        if opts.get("exclude"):
            args += ["--exclude", opts["exclude"]]

        return args

    def _log_cmd(self, cmd: list[str]) -> None:
        """Log command without exposing passwords."""
        safe = []
        skip_next = False
        for arg in cmd:
            if skip_next:
                safe.append("***")
                skip_next = False
            elif arg in ("--pass", "--code"):
                safe.append(arg)
                skip_next = True
            else:
                safe.append(arg)
        logger.info("Running: %s", " ".join(safe))

    # ── Internal: run croc subprocess ──

    def _run_send(self, paths: list[str], opts: dict) -> None:
        proc = None
        file_names = [os.path.basename(p) for p in paths]
        success = False
        error_msg = ""
        try:
            cmd = self._build_global_args(opts) + self._build_send_args(opts) + paths
            self._log_cmd(cmd)
            proc = subprocess.Popen(
                cmd,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                creationflags=_NO_WINDOW,
            )
            with self._lock:
                self._process = proc
            last_sending_line = ""
            for line in proc.stdout:
                line = line.rstrip()
                if not line:
                    continue
                if "Code is:" in line:
                    code = line.split("Code is:")[-1].strip()
                    self._js_event("code_ready", {"code": code})
                # Suppress noisy incremental "Sending N files" lines from croc
                if line.startswith("Sending ") and "files" in line:
                    last_sending_line = line
                    continue
                if last_sending_line:
                    self._js_log("info", last_sending_line)
                    last_sending_line = ""
                self._js_log("info", line)
            if last_sending_line:
                self._js_log("info", last_sending_line)
            proc.wait(timeout=10)
            success = proc.returncode == 0
        except subprocess.TimeoutExpired:
            error_msg = "Transfer timed out"
        except Exception as e:
            logger.error("Send error: %s", e)
            error_msg = str(e)
        finally:
            _safe_close_process(proc)
            with self._lock:
                stopped = self._process is None
                self._process = None
            if stopped:
                return  # stop_transfer already handled events
            if error_msg:
                self._js_log("error", error_msg)
            self._js_event(
                "transfer_done",
                {"success": success, "files": file_names, "mode": "send"},
            )

    def _run_send_text(self, text: str, opts: dict) -> None:
        proc = None
        success = False
        error_msg = ""
        try:
            cmd = (
                self._build_global_args(opts)
                + self._build_send_args(opts)
                + ["--text", text]
            )
            self._log_cmd(cmd)
            proc = subprocess.Popen(
                cmd,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                creationflags=_NO_WINDOW,
            )
            with self._lock:
                self._process = proc
            for line in proc.stdout:
                line = line.rstrip()
                if not line:
                    continue
                if "Code is:" in line:
                    code = line.split("Code is:")[-1].strip()
                    self._js_event("code_ready", {"code": code})
                self._js_log("info", line)
            proc.wait(timeout=10)
            success = proc.returncode == 0
        except subprocess.TimeoutExpired:
            error_msg = "Transfer timed out"
        except Exception as e:
            logger.error("Send text error: %s", e)
            error_msg = str(e)
        finally:
            _safe_close_process(proc)
            with self._lock:
                stopped = self._process is None
                self._process = None
            if stopped:
                return
            if error_msg:
                self._js_log("error", error_msg)
            self._js_event(
                "transfer_done",
                {"success": success, "files": ["(text)"], "mode": "send"},
            )

    def _run_receive(self, code: str, opts: dict) -> None:
        proc = None
        received_files: list[str] = []
        success = False
        error_msg = ""
        try:
            cmd = self._build_global_args(opts)
            if opts.get("outFolder"):
                cmd += ["--out", opts["outFolder"]]
            cmd.append(code)
            self._log_cmd(cmd)
            proc = subprocess.Popen(
                cmd,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                creationflags=_NO_WINDOW,
            )
            with self._lock:
                self._process = proc
            for line in proc.stdout:
                line = line.rstrip()
                if not line:
                    continue
                if line.startswith("Receiving '") and "'" in line[11:]:
                    fname = line[11 : line.index("'", 11)]
                    if fname and fname not in received_files:
                        received_files.append(fname)
                self._js_log("info", line)
            proc.wait(timeout=10)
            success = proc.returncode == 0
        except subprocess.TimeoutExpired:
            error_msg = "Transfer timed out"
        except Exception as e:
            logger.error("Receive error: %s", e)
            error_msg = str(e)
        finally:
            _safe_close_process(proc)
            with self._lock:
                stopped = self._process is None
                self._process = None
            if stopped:
                return
            if error_msg:
                self._js_log("error", error_msg)
            if success:
                self._notify("receive_done")
            self._js_event(
                "transfer_done",
                {"success": success, "files": received_files, "mode": "receive"},
            )

    # ── Thumbnail API ──

    def get_thumbnail(self, path: str, max_size: int = 200) -> str | None:
        """Return a base64 data URI for an image file, or None."""
        path = os.path.normpath(path)
        if not os.path.isfile(path):
            return None
        ext = os.path.splitext(path)[1].lower()
        mime = mimetypes.guess_type(path)[0]
        if not mime or not mime.startswith("image/"):
            # SVG special case
            if ext == ".svg":
                mime = "image/svg+xml"
            else:
                return None
        try:
            with open(path, "rb") as f:
                data = f.read(10 * 1024 * 1024)  # Max 10MB
            encoded = base64.b64encode(data).decode("ascii")
            return f"data:{mime};base64,{encoded}"
        except Exception as e:
            logger.warning("Thumbnail failed for %s: %s", path, e)
            return None

    # ── JS bridge helpers ──

    def _js_log(self, level: str, msg: str) -> None:
        if self._window:
            safe = json.dumps(msg)
            self._window.evaluate_js(f'window.onLog && window.onLog("{level}", {safe})')

    def _js_event(self, event: str, data: dict) -> None:
        if self._window:
            safe = json.dumps(data)
            self._window.evaluate_js(
                f'window.onEvent && window.onEvent("{event}", {safe})'
            )
