"""Croc Transfer — pywebview bootstrap.

Requires Python 3.14+.

Creates a native desktop window and exposes the Python backend API
to the Svelte frontend via window.pywebview.api.

Usage:
    python main.py          # Production — loads dist/index.html
    python main.py --dev    # Development — loads Vite dev server (hot reload)
"""

import ctypes
import logging
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import webview  # noqa: E402

from backend.croc_api import CrocAPI  # noqa: E402

DEV_URL = "http://localhost:5173"
logger = logging.getLogger(__name__)

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(name)s %(message)s",
    datefmt="%H:%M:%S",
)

_ORIGIN_FILE = os.path.join(
    os.environ.get("APPDATA", tempfile.gettempdir()),
    "CrocTransfer",
    "origin.txt",
)

# ── Single-Instance Guard (Windows named mutex via ctypes) ──

_MUTEX_NAME = "CrocTransfer_SingleInstance_D0E858DF"
_mutex_handle = None


def _acquire_single_instance() -> bool:
    """Try to acquire a system-wide named mutex.

    Returns True if this is the only instance.
    Returns False if another instance already holds the mutex.
    On non-Windows, always returns True (no mutex support needed).
    """
    global _mutex_handle
    if sys.platform != "win32":
        return True

    kernel32 = ctypes.windll.kernel32
    _mutex_handle = kernel32.CreateMutexW(None, False, _MUTEX_NAME)
    error = kernel32.GetLastError()
    ERROR_ALREADY_EXISTS = 183

    if error == ERROR_ALREADY_EXISTS:
        # Another instance is running — bring it to front if possible
        kernel32.CloseHandle(_mutex_handle)
        _mutex_handle = None
        return False

    return True


def _release_single_instance() -> None:
    """Release the named mutex on exit."""
    global _mutex_handle
    if _mutex_handle and sys.platform == "win32":
        ctypes.windll.kernel32.CloseHandle(_mutex_handle)
        _mutex_handle = None


def _focus_existing_window() -> None:
    """Try to bring the existing CrocTransfer window to front."""
    if sys.platform != "win32":
        return
    try:
        user32 = ctypes.windll.user32
        hwnd = user32.FindWindowW(None, "Croc Transfer")
        if hwnd:
            SW_RESTORE = 9
            user32.ShowWindow(hwnd, SW_RESTORE)
            user32.SetForegroundWindow(hwnd)
    except Exception:
        pass


# ── Self-install / update helpers ──


def _is_temp_path(path: str) -> bool:
    """Check if a path is inside a temp directory."""
    tmp = tempfile.gettempdir().lower()
    return os.path.normpath(path).lower().startswith(os.path.normpath(tmp).lower())


def _save_origin(path: str) -> None:
    """Remember where the exe lives so updates can replace it."""
    os.makedirs(os.path.dirname(_ORIGIN_FILE), exist_ok=True)
    with open(_ORIGIN_FILE, "w") as f:
        f.write(path)


def _read_origin() -> str | None:
    """Read the saved original exe path."""
    try:
        with open(_ORIGIN_FILE) as f:
            path = f.read().strip()
            return path if path else None
    except Exception:
        return None


def _self_install() -> bool:
    """If running from temp (launched by old updater), replace the original exe."""
    if not getattr(sys, "frozen", False):
        return False

    current = sys.executable
    if not _is_temp_path(current):
        _save_origin(current)
        return False

    origin = _read_origin()
    if origin:
        for attempt in range(10):
            try:
                shutil.copy2(current, origin)
                if os.path.getsize(origin) == os.path.getsize(current):
                    logger.info("Updated original exe at: %s", origin)
                    subprocess.Popen([origin])
                    return True
                logger.warning("Size mismatch, retrying...")
            except Exception as e:
                logger.warning("Copy attempt %d failed: %s", attempt + 1, e)
            time.sleep(2)
        logger.error("Failed to replace original exe after 10 attempts")

    return False


def _kill_all_croc() -> None:
    """Kill any remaining croc.exe processes on Windows."""
    if sys.platform != "win32":
        return
    try:
        subprocess.run(
            ["taskkill", "/F", "/IM", "croc.exe"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except Exception:
        pass


def main():
    if _self_install():
        sys.exit(0)

    # Single-instance check — exit if another CrocTransfer is running
    if not _acquire_single_instance():
        logger.info("Another instance is already running, focusing it")
        _focus_existing_window()
        sys.exit(0)

    import atexit

    atexit.register(_release_single_instance)

    dev_mode = "--dev" in sys.argv
    api = CrocAPI()

    if dev_mode:
        url = DEV_URL
    else:
        dist_path = Path(__file__).parent / "dist" / "index.html"
        url = str(dist_path) if dist_path.exists() else DEV_URL

    window = webview.create_window(
        title="Croc Transfer",
        url=url,
        js_api=api,
        width=520,
        height=720,
        min_size=(420, 550),
        background_color="#141218",
        text_select=True,
    )

    api.set_window(window)

    def _on_closing():
        """Hard-kill everything when the window X is pressed."""
        api._cleanup()
        _kill_all_croc()
        _release_single_instance()
        os._exit(0)

    window.events.closing += _on_closing

    storage = os.path.join(
        os.environ.get("APPDATA", tempfile.gettempdir()),
        "CrocTransfer",
    )
    webview.start(debug=dev_mode, private_mode=False, storage_path=storage)

    # Fallback if closing event didn't fire
    api._cleanup()
    _kill_all_croc()


if __name__ == "__main__":
    main()
