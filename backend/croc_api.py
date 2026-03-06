"""
Croc Transfer — Python backend API.
Exposed to the Svelte frontend via window.pywebview.api.
"""

import glob as _glob
import json
import logging
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import webbrowser

import webview

from backend.lan_server import LANPeer
from backend.updater import check_for_updates, download_update
from backend.version import APP_VERSION

logger = logging.getLogger(__name__)

# Avoid console windows popping up on Windows
_NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)


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
        except Exception:
            pass


class CrocAPI:
    """pywebview js_api — each public method becomes window.pywebview.api.<name>()

    IMPORTANT: All non-API attributes MUST start with _ to prevent pywebview
    from recursively enumerating them (which causes infinite recursion on
    WinForms .NET AccessibilityObject properties).
    """

    def __init__(self) -> None:
        self._window: webview.Window | None = None
        self._lock = threading.Lock()  # Protects _process and _transfer_active
        self._process: subprocess.Popen | None = None
        self._transfer_active = False
        self._croc_path = self._find_croc()
        # Auto-receive background listener
        self._auto_lock = threading.Lock()  # Protects auto-receive state
        self._auto_recv_process: subprocess.Popen | None = None
        self._auto_recv_running = False
        self._auto_recv_code: str | None = None
        self._auto_recv_opts: dict = {}
        self._auto_recv_paused = False
        # Local relay server
        self._relay_process: subprocess.Popen | None = None
        self._relay_port = 9009
        # LAN direct transfer
        self._lan_peer: LANPeer | None = None
        self._lan_peer_lock = threading.Lock()

    def set_window(self, window: webview.Window) -> None:
        self._window = window

    # ── Local relay server ──

    def _start_relay(self) -> None:
        """Start a local croc relay so auto-receive can hold rooms open."""
        if self._relay_process and self._relay_process.poll() is None:
            return  # Already running
        if not self._croc_path:
            return

        # Find a free port starting from 9009
        port = 9009
        for _attempt in range(10):
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.bind(("127.0.0.1", port))
                    break
            except OSError:
                port += 10  # Try 9009, 9019, 9029...
        else:
            logger.error("Could not find free port for relay")
            return

        self._relay_port = port
        # Relay uses 5 consecutive ports (port, port+1, ..., port+4)
        ports = ",".join(str(port + i) for i in range(5))
        cmd = [self._croc_path, "relay", "--host", "0.0.0.0", "--ports", ports]
        logger.info("Starting local relay on port %d: %s", port, " ".join(cmd))
        self._relay_process = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=_NO_WINDOW,
        )
        # Give the relay a moment to start
        time.sleep(0.5)
        if self._relay_process.poll() is not None:
            logger.error(
                "Relay process exited immediately (port %d may be in use)", port
            )
            self._relay_process = None
        else:
            logger.info(
                "Local relay running on port %d (PID %d)", port, self._relay_process.pid
            )

    def _stop_relay(self) -> None:
        """Stop the local relay server."""
        proc = self._relay_process
        self._relay_process = None
        _safe_close_process(proc)

    def _relay_addr(self) -> str:
        """Return the local relay address string for croc --relay flag."""
        return f"localhost:{self._relay_port}"

    # ── Croc discovery ──

    @staticmethod
    def _find_croc() -> str | None:

        found = shutil.which("croc")
        if found:
            return found

        # Build candidate paths from multiple env vars (some may be missing
        # in PyInstaller-packaged context)
        local_app = os.environ.get("LOCALAPPDATA", "")
        user_profile = os.environ.get("USERPROFILE", "")
        candidates: list[str] = []

        # WinGet actual package install (direct binary — most reliable)
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

        # WinGet Links symlink (app execution alias — may not resolve via isfile)
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

        # Scoop
        if user_profile:
            candidates.append(os.path.join(user_profile, "scoop", "shims", "croc.exe"))

        # Chocolatey
        candidates.append(r"C:\ProgramData\chocolatey\bin\croc.exe")

        # Manual install in Program Files
        candidates.append(r"C:\Program Files\croc\croc.exe")
        candidates.append(r"C:\Program Files (x86)\croc\croc.exe")

        # Linux/macOS
        candidates.append(os.path.expanduser("~/.local/bin/croc"))
        candidates.append("/usr/local/bin/croc")

        for p in candidates:
            if p and os.path.isfile(p):
                logger.info("Found croc at: %s", p)
                return p

        # Glob fallback: search WinGet Packages for any croc package variant
        for base in [
            local_app,
            os.path.join(user_profile, "AppData", "Local") if user_profile else "",
        ]:
            if not base:
                continue
            pattern = os.path.join(
                base, "Microsoft", "WinGet", "Packages", "schollz.croc*", "croc.exe"
            )
            matches = _glob.glob(pattern)
            if matches:
                logger.info("Found croc via glob: %s", matches[0])
                return matches[0]

        logger.warning(
            "croc not found. Searched PATH and %d candidate locations. "
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
            # Connect to a public address to determine which interface is used
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
        """Download update .exe in background, report progress to frontend."""

        def _run():
            def on_progress(pct):
                self._js_event("update_progress", {"percent": pct})

            try:
                path = download_update(url, progress_callback=on_progress)
                if path:
                    self._js_event(
                        "update_ready",
                        {"path": path, "file_name": os.path.basename(path)},
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

        threading.Thread(target=_run, daemon=True).start()

    def launch_update(self, path: str) -> None:
        """Replace the current exe with the downloaded update and restart.

        Uses a PowerShell script that:
        1. Waits until the exe file is unlocked (not just PID — PyInstaller
           bootloader parent holds the lock longer than the child)
        2. Copies the new exe over the original
        3. Verifies file size
        4. Launches the updated exe
        5. Writes a debug log for troubleshooting
        """
        if not os.path.isfile(path):
            return

        new_size = os.path.getsize(path)
        current_exe = sys.executable
        log_path = os.path.join(tempfile.gettempdir(), "crude_update_log.txt")

        # Only do in-place replacement for PyInstaller-bundled exe
        if getattr(sys, "frozen", False) and current_exe.endswith(".exe"):
            ps1 = os.path.join(tempfile.gettempdir(), "crude_update.ps1")
            # PowerShell script — waits for file to be unlocked, not PID
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

# Wait until the exe is unlocked (up to 30 seconds)
# PyInstaller onefile: parent bootloader holds the lock after child exits
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

# Retry copy up to 10 times
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

# Cleanup
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
            # Dev mode fallback — just launch the new exe
            os.startfile(path)

        # Clean up auto-receive before exiting
        self._stop_auto_recv_internal()

        if self._window:
            self._window.destroy()
        sys.exit(0)

    def install_croc(self) -> None:
        """Install croc via winget in background, report progress to frontend."""

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
                    line = line.rstrip()
                    if line:
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
                    "winget not found. Install croc manually: https://github.com/schollz/croc#install",
                )
                self._js_event(
                    "install_croc_done", {"success": False, "no_winget": True}
                )
            except (subprocess.TimeoutExpired, OSError) as e:
                self._js_log("error", f"Installation failed: {e}")
                self._js_event("install_croc_done", {"success": False})
            finally:
                _safe_close_process(proc)

        threading.Thread(target=_run, daemon=True).start()

    def open_url(self, url: str) -> None:
        """Open a URL in the default browser."""
        webbrowser.open(url)

    def open_folder(self, path: str) -> bool:
        """Open a folder in the system file explorer."""
        path = os.path.normpath(path)
        if os.path.isfile(path):
            # If it's a file, open its parent folder
            path = os.path.dirname(path)
        if not os.path.isdir(path):
            return False
        if sys.platform == "win32":
            os.startfile(path)
        elif sys.platform == "darwin":
            subprocess.Popen(["open", path])
        else:
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
        # Normalize and validate path — reject path traversal
        path = os.path.normpath(path)
        if not os.path.exists(path):
            return None
        name = os.path.basename(path) or path
        is_dir = os.path.isdir(path)
        if is_dir:
            total = 0
            count = 0
            for root, dirs, files in os.walk(path):
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
        else:
            size = os.path.getsize(path)
            ext = os.path.splitext(name)[1].lower()
            return {"name": name, "size": _file_size_str(size), "type": ext or "file"}

    def send_files(self, paths: list, options: dict | None = None) -> None:
        """Send one or more files/folders with optional croc flags."""
        if not self._croc_path:
            self._js_log("error", "croc not found!")
            return
        valid = [p for p in paths if os.path.exists(p)]
        if not valid:
            self._js_log("error", "No valid files selected")
            return

        # Pause auto-receive to free the relay room
        self._pause_auto_receive()

        with self._lock:
            self._transfer_active = True
        names = [os.path.basename(p) for p in valid]
        self._js_event("transfer_start", {"mode": "send", "files": names})
        threading.Thread(
            target=self._run_send, args=(valid, options or {}), daemon=True
        ).start()

    def send_text(self, text: str, options: dict | None = None) -> None:
        """Send text content via croc."""
        if not self._croc_path:
            self._js_log("error", "croc not found!")
            return
        text = (text or "").strip()
        if not text:
            self._js_log("error", "No text provided")
            return

        # Pause auto-receive to free the relay room
        self._pause_auto_receive()

        with self._lock:
            self._transfer_active = True
        self._js_event("transfer_start", {"mode": "send", "files": ["(text)"]})
        threading.Thread(
            target=self._run_send_text, args=(text, options or {}), daemon=True
        ).start()

    def receive_file(self, code: str, options: dict | None = None) -> None:
        if not self._croc_path:
            self._js_log("error", "croc not found!")
            return
        code = (code or "").strip()
        if not code:
            self._js_log("error", "No code provided")
            return

        # Pause auto-receive to free the relay room
        self._pause_auto_receive()

        with self._lock:
            self._transfer_active = True
        self._js_event("transfer_start", {"mode": "receive", "code": code})
        threading.Thread(
            target=self._run_receive, args=(code, options or {}), daemon=True
        ).start()

    def stop_transfer(self) -> bool:
        with self._lock:
            proc = self._process
            if proc:
                try:
                    proc.terminate()
                except Exception:
                    pass
                self._transfer_active = False
                self._js_log("warn", "Transfer stopped")
                self._js_event("transfer_done", {"success": False, "stopped": True})
                return True
        return False

    # ── Auto-receive (background listener) ──

    def start_auto_receive(self, code: str, options: dict | None = None) -> None:
        """Start a background loop that keeps croc listening for incoming transfers.
        Uses a local croc relay so the receiver can hold the room open waiting for a sender."""
        if not self._croc_path:
            self._js_log("error", "croc not found!")
            return
        code = (code or "").strip()
        if not code:
            return

        # Stop any existing auto-receive first
        self._stop_auto_recv_internal()

        # Start local relay — this lets the receiver hold a room open
        self._start_relay()

        with self._auto_lock:
            self._auto_recv_running = True
            self._auto_recv_code = code
            self._auto_recv_opts = options or {}
        threading.Thread(target=self._auto_recv_loop, daemon=True).start()
        self._js_event("auto_receive_started", {"code": code})
        self._js_log("info", f"Auto-receive listening on code '{code}'")

    def stop_auto_receive(self) -> None:
        """Stop the background auto-receive listener and local relay."""
        with self._auto_lock:
            was_running = self._auto_recv_running
        self._stop_auto_recv_internal()
        self._stop_relay()
        if was_running:
            self._js_event("auto_receive_stopped", {})
            self._js_log("info", "Auto-receive stopped")

    def _stop_auto_recv_internal(self) -> None:
        with self._auto_lock:
            self._auto_recv_running = False
            self._auto_recv_paused = False
            proc = self._auto_recv_process
            self._auto_recv_process = None
        _safe_close_process(proc)

    def _pause_auto_receive(self) -> None:
        """Temporarily stop auto-receive so the relay room is free for manual transfers."""
        with self._auto_lock:
            if not self._auto_recv_running:
                return
            self._auto_recv_paused = True
            self._auto_recv_running = False
            proc = self._auto_recv_process
            self._auto_recv_process = None
        _safe_close_process(proc)
        # Give relay time to release the room
        time.sleep(1)
        self._js_log("info", "Auto-receive paused for transfer")

    def _resume_auto_receive(self) -> None:
        """Restart auto-receive if it was paused by a manual transfer."""
        with self._auto_lock:
            if not self._auto_recv_paused:
                return
            self._auto_recv_paused = False
            code = self._auto_recv_code
            opts = self._auto_recv_opts.copy()
        if code:
            self._js_log("info", "Resuming auto-receive")
            self.start_auto_receive(code, opts)

    # Errors that mean "no sender is connected yet" — expected, don't spam the log
    _QUIET_ERRORS = (
        "not ready",
        "peer disconnected",
        "no rooms",
        "room is full",
        "problem with decoding",
    )

    def _auto_recv_loop(self) -> None:
        """Continuously run croc receive, restarting after each completed transfer."""
        consecutive_quiet = 0  # Track silent "no sender" failures
        while True:
            with self._auto_lock:
                if not self._auto_recv_running:
                    break
                code = self._auto_recv_code
                opts = self._auto_recv_opts.copy()

            proc = None
            received_files: list[str] = []
            received_text: str | None = None
            got_data = False  # True once we see "Receiving" (a real transfer started)
            is_text = False  # True when receiving text message
            try:
                cmd = self._build_global_args(opts)
                # Use local relay so the room stays open waiting for sender
                if self._relay_process and self._relay_process.poll() is None:
                    cmd += ["--relay", self._relay_addr()]
                if opts.get("outFolder"):
                    cmd += ["--out", opts["outFolder"]]
                cmd.append(code)

                logger.info("Auto-receive: listening...")
                proc = subprocess.Popen(
                    cmd,
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    bufsize=1,
                    creationflags=_NO_WINDOW,
                )
                with self._auto_lock:
                    self._auto_recv_process = proc

                text_lines: list[str] = []
                capture_text = False

                for line in proc.stdout:
                    line = line.rstrip()
                    if not line:
                        continue

                    # Detect text message: croc outputs "Receiving text message"
                    if "Receiving text" in line or "receiving text" in line:
                        got_data = True
                        is_text = True
                        capture_text = True
                        continue

                    # Capture text content lines (after "Receiving text" header)
                    if capture_text and not line.startswith("Receiving "):
                        # Skip progress lines and croc status
                        if "%" not in line and "|" not in line and "MB" not in line:
                            text_lines.append(line)

                    if line.startswith("Receiving '") and "'" in line[11:]:
                        fname = line[11 : line.index("'", 11)]
                        if fname and fname not in received_files:
                            received_files.append(fname)
                        got_data = True

                    # Only show log lines to UI once a real transfer starts
                    if got_data and not capture_text:
                        self._js_log("info", f"[auto] {line}")
                    elif not got_data:
                        logger.debug("Auto-receive: %s", line)

                if text_lines:
                    received_text = "\n".join(text_lines)

                proc.wait(timeout=3600)
                success = proc.returncode == 0

                with self._auto_lock:
                    self._auto_recv_process = None

                if success and got_data:
                    consecutive_quiet = 0
                    if is_text and received_text:
                        self._js_event(
                            "auto_receive_done",
                            {
                                "success": True,
                                "files": ["(text)"],
                                "text": received_text,
                            },
                        )
                        self._js_log("success", "Auto-receive: text received!")
                    else:
                        self._js_event(
                            "auto_receive_done",
                            {
                                "success": True,
                                "files": received_files,
                            },
                        )
                        names = ", ".join(received_files) if received_files else "file"
                        self._js_log("success", f"Auto-receive: received {names}!")
                elif not success and self._auto_recv_running:
                    if got_data:
                        consecutive_quiet = 0
                        self._js_log(
                            "warn", "Auto-receive: transfer failed, retrying..."
                        )
                    else:
                        consecutive_quiet += 1
                        logger.debug(
                            "Auto-receive: no sender (attempt %d)", consecutive_quiet
                        )

            except subprocess.TimeoutExpired:
                logger.warning("Auto-receive: process timed out, restarting")
            except Exception as e:
                logger.error("Auto-receive error: %s", e)
            finally:
                _safe_close_process(proc)
                with self._auto_lock:
                    self._auto_recv_process = None
                    if not self._auto_recv_running:
                        break

            # Back off: short wait normally, longer after many silent failures
            wait = min(5 + consecutive_quiet * 2, 30)
            time.sleep(wait)

    # ── LAN Direct Transfer ──

    def start_lan(self, code: str, out_folder: str = "") -> None:
        """Start LAN direct transfer for a contact code.
        Automatically discovers peers on the same network with the same code."""
        code = (code or "").strip()
        if not code:
            return
        self._stop_lan_internal()

        def on_event(event: str, data: dict) -> None:
            self._js_event(event, data)

        def on_log(level: str, msg: str) -> None:
            self._js_log(level, msg)

        peer = LANPeer(code, on_event, on_log)
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
        """Send text instantly over LAN. Returns True if sent, False to fall back to croc."""
        with self._lan_peer_lock:
            peer = self._lan_peer
        if peer and peer.connected:
            return peer.send_text(text)
        return False

    def lan_send_files(self, paths: list, out_folder: str = "") -> bool:
        """Send files instantly over LAN. Returns True if sent, False to fall back to croc."""
        with self._lan_peer_lock:
            peer = self._lan_peer
        if peer and peer.connected:
            success = peer.send_files(paths)
            if success:
                names = [os.path.basename(p) for p in paths]
                self._js_event(
                    "transfer_done",
                    {"success": True, "files": names, "mode": "send"},
                )
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

    # ── Internal: build CLI args from options ──

    def _build_global_args(self, opts: dict) -> list[str]:
        """Build global croc flags from options dict.
        Note: 'sourceFolder' is a UI-only key, ignored here.
        """
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
        """Build send subcommand flags from options dict."""
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
            for line in proc.stdout:
                line = line.rstrip()
                if not line:
                    continue
                if "Code is:" in line:
                    code = line.split("Code is:")[-1].strip()
                    self._js_event("code_ready", {"code": code})
                self._js_log("info", line)
            proc.wait(timeout=3600)
            success = proc.returncode == 0
            self._js_event(
                "transfer_done",
                {
                    "success": success,
                    "files": file_names,
                    "mode": "send",
                },
            )
        except subprocess.TimeoutExpired:
            self._js_log("error", "Transfer timed out")
            self._js_event("transfer_done", {"success": False})
        except Exception as e:
            logger.error("Send error: %s", e)
            self._js_log("error", str(e))
            self._js_event("transfer_done", {"success": False})
        finally:
            _safe_close_process(proc)
            with self._lock:
                self._process = None
                self._transfer_active = False
            self._resume_auto_receive()

    def _run_send_text(self, text: str, opts: dict) -> None:
        proc = None
        try:
            cmd = (
                self._build_global_args(opts)
                + self._build_send_args(opts)
                + ["--text", text]
            )
            logger.info("Running: croc send --text ...")
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
            proc.wait(timeout=3600)
            success = proc.returncode == 0
            self._js_event(
                "transfer_done",
                {
                    "success": success,
                    "files": ["(text)"],
                    "mode": "send",
                },
            )
        except subprocess.TimeoutExpired:
            self._js_log("error", "Transfer timed out")
            self._js_event("transfer_done", {"success": False})
        except Exception as e:
            logger.error("Send text error: %s", e)
            self._js_log("error", str(e))
            self._js_event("transfer_done", {"success": False})
        finally:
            _safe_close_process(proc)
            with self._lock:
                self._process = None
                self._transfer_active = False
            self._resume_auto_receive()

    def _run_receive(self, code: str, opts: dict) -> None:
        proc = None
        received_files: list[str] = []
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
                # Parse file names from croc output like:
                # "Receiving 'filename.txt' (1.2 MB)"
                # or progress lines: "filename.txt 100% |████| (1.2/1.2 MB)"
                if line.startswith("Receiving '") and "'" in line[11:]:
                    fname = line[11 : line.index("'", 11)]
                    if fname and fname not in received_files:
                        received_files.append(fname)
                self._js_log("info", line)
            proc.wait(timeout=3600)
            success = proc.returncode == 0
            self._js_event(
                "transfer_done",
                {
                    "success": success,
                    "files": received_files,
                    "mode": "receive",
                },
            )
        except subprocess.TimeoutExpired:
            self._js_log("error", "Transfer timed out")
            self._js_event("transfer_done", {"success": False})
        except Exception as e:
            logger.error("Receive error: %s", e)
            self._js_log("error", str(e))
            self._js_event("transfer_done", {"success": False})
        finally:
            _safe_close_process(proc)
            with self._lock:
                self._process = None
                self._transfer_active = False
            self._resume_auto_receive()

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
