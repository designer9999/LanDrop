"""
GitHub-based auto-updater.
Checks for new releases, downloads .exe updates, launches installer.

Requires Python 3.14+.
"""

import json
import logging
import os
import re
import tempfile
import urllib.request
from collections.abc import Callable

from backend.version import APP_VERSION, GITHUB_REPO

logger = logging.getLogger(__name__)

API_URL = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"


def _parse_version(tag: str) -> tuple[int, ...]:
    """Parse 'v1.2.3' or '1.2.3' into (1, 2, 3)."""
    clean = tag.lstrip("vV").strip()
    parts: list[int] = []
    for p in clean.split("."):
        try:
            parts.append(int(p))
        except ValueError:
            break
    return tuple(parts) or (0,)


def _sanitize_filename(name: str) -> str:
    """Sanitize a filename — strip path separators and traversal."""
    name = os.path.basename(name)
    name = re.sub(r'[<>:"|?*]', "_", name)
    if not name or name.startswith("."):
        name = "CrocTransfer-update.exe"
    return name


def check_for_updates() -> dict:
    """Check GitHub for a newer release."""
    result: dict = {"update_available": False, "current_version": APP_VERSION}

    try:
        req = urllib.request.Request(
            API_URL,
            headers={
                "Accept": "application/vnd.github.v3+json",
                "User-Agent": "Crude-Updater",
            },
        )
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read())

        latest_tag = data.get("tag_name", "")
        result["latest_version"] = latest_tag

        if body := data.get("body", ""):
            result["release_notes"] = body[:500]

        current = _parse_version(APP_VERSION)
        latest = _parse_version(latest_tag)

        if latest > current:
            result["update_available"] = True

            for asset in data.get("assets", []):
                name = asset.get("name", "")
                if name.endswith(".exe"):
                    result["download_url"] = asset["browser_download_url"]
                    result["file_name"] = name
                    result["file_size"] = asset.get("size", 0)
                    break

            if "download_url" not in result:
                result["download_url"] = data.get("html_url", "")

    except Exception as e:
        logger.error("Update check failed: %s", e)
        result["error"] = str(e)

    return result


def download_update(
    url: str, progress_callback: Callable[[int], None] | None = None
) -> str | None:
    """Download the update .exe to a temp folder."""
    dest = None
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "Crude-Updater"})
        with urllib.request.urlopen(req, timeout=300) as resp:
            total = int(resp.headers.get("Content-Length", 0))
            raw_name = url.split("/")[-1]
            file_name = _sanitize_filename(raw_name)
            if not file_name.endswith(".exe"):
                file_name = "CrocTransfer-update.exe"

            dest = os.path.join(tempfile.gettempdir(), file_name)
            downloaded = 0
            chunk_size = 64 * 1024

            with open(dest, "wb") as f:
                while chunk := resp.read(chunk_size):
                    f.write(chunk)
                    downloaded += len(chunk)
                    if total > 0 and progress_callback:
                        pct = min(100, int(downloaded * 100 / total))
                        progress_callback(pct)

            if progress_callback:
                progress_callback(100)

            actual_size = os.path.getsize(dest)
            if actual_size < 1_000_000:
                logger.error("Downloaded file too small: %d bytes", actual_size)
                os.unlink(dest)
                return None

            logger.info("Update downloaded to: %s (%d bytes)", dest, actual_size)
            return dest

    except Exception as e:
        logger.error("Download failed: %s (%s)", e, type(e).__name__)
        if dest and os.path.isfile(dest):
            try:
                os.unlink(dest)
            except OSError:
                pass
        raise
