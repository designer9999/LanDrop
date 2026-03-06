"""Croc Transfer — pywebview bootstrap.

Requires Python 3.14+.

Creates a native desktop window and exposes the Python backend API
to the Svelte frontend via window.pywebview.api.

Usage:
    python main.py          # Production — loads dist/index.html
    python main.py --dev    # Development — loads Vite dev server (hot reload)
"""

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


def main():
    if _self_install():
        sys.exit(0)

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

    storage = os.path.join(
        os.environ.get("APPDATA", tempfile.gettempdir()),
        "CrocTransfer",
    )
    webview.start(debug=dev_mode, private_mode=False, storage_path=storage)


if __name__ == "__main__":
    main()
