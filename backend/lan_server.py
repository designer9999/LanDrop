"""
LAN Direct Transfer — instant file & text transfer over local network.

Requires Python 3.14+.

Architecture:
  - UDP broadcast beacon for automatic peer discovery (port 29170)
  - TCP server for persistent connections (port 29171)
  - Protocol: length-prefixed JSON headers + raw binary file data

Each peer advertises itself via UDP broadcast every 2s with its code phrase
(hashed). When a matching peer is found, a persistent TCP connection is
established. Text messages and file transfers flow over this connection
with near-zero latency.

Wire protocol:
  MSG_TEXT:    { "type": "text", "text": "..." }
  MSG_FILE:    { "type": "file", "name": "...", "size": N }  + N bytes raw data
  MSG_DIR:     { "type": "dir",  "name": "..." }  (create directory)
  MSG_BATCH:   { "type": "batch", "count": N }  (N files follow)
  MSG_DONE:    { "type": "done" }  (batch complete)
  MSG_PING:    { "type": "ping" }
  MSG_PONG:    { "type": "pong" }
"""

import hashlib
import json
import logging
import os
import re
import socket
import struct
import subprocess
import sys
import threading
import time
from collections.abc import Callable
from typing import BinaryIO

logger = logging.getLogger(__name__)

_NO_WINDOW = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0

BEACON_PORT = 29170
TRANSFER_PORT = 29171
BEACON_MAGIC = b"CRUDE1"
HEADER_FMT = "!I"  # 4-byte big-endian length prefix
CHUNK_SIZE = 256 * 1024  # 256KB read chunks
BEACON_INTERVAL = 2.0
PING_INTERVAL = 10.0
CONNECT_TIMEOUT = 5.0

_FIREWALL_ADDED = False


def _ensure_firewall_rules() -> None:
    """Add Windows Firewall rules for LAN direct transfer (UDP + TCP)."""
    global _FIREWALL_ADDED
    if _FIREWALL_ADDED or sys.platform != "win32":
        return
    _FIREWALL_ADDED = True

    exe = sys.executable if getattr(sys, "frozen", False) else None
    if not exe:
        return

    for proto, port, name_suffix in [
        ("UDP", BEACON_PORT, "Beacon"),
        ("TCP", TRANSFER_PORT, "Transfer"),
    ]:
        rule_name = f"CrocTransfer LAN {name_suffix}"
        try:
            check = subprocess.run(
                [
                    "netsh",
                    "advfirewall",
                    "firewall",
                    "show",
                    "rule",
                    f"name={rule_name}",
                ],
                capture_output=True,
                text=True,
                creationflags=_NO_WINDOW,
                timeout=5,
            )
            if check.returncode == 0 and rule_name in check.stdout:
                continue

            subprocess.run(
                [
                    "netsh",
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    f"name={rule_name}",
                    "dir=in",
                    "action=allow",
                    f"protocol={proto}",
                    f"localport={port}",
                    f"program={exe}",
                ],
                capture_output=True,
                creationflags=_NO_WINDOW,
                timeout=5,
            )
            logger.info("Firewall rule added: %s", rule_name)
        except Exception as e:
            logger.debug("Firewall rule %s failed: %s", rule_name, e)


def _hash_code(code: str) -> str:
    """Hash the code phrase for safe broadcast."""
    return hashlib.sha256(code.encode()).hexdigest()[:16]


def _send_msg(sock: socket.socket, data: dict) -> None:
    """Send a length-prefixed JSON message."""
    payload = json.dumps(data, separators=(",", ":")).encode()
    sock.sendall(struct.pack(HEADER_FMT, len(payload)) + payload)


def _recv_msg(sock: socket.socket) -> dict | None:
    """Receive a length-prefixed JSON message. Returns None on disconnect."""
    header = bytearray(4)
    pos = 0
    while pos < 4:
        chunk = sock.recv(4 - pos)
        if not chunk:
            return None
        header[pos : pos + len(chunk)] = chunk
        pos += len(chunk)
    (length,) = struct.unpack(HEADER_FMT, header)
    if length > 100 * 1024 * 1024:
        return None
    buf = bytearray(length)
    pos = 0
    while pos < length:
        chunk = sock.recv(min(length - pos, CHUNK_SIZE))
        if not chunk:
            return None
        buf[pos : pos + len(chunk)] = chunk
        pos += len(chunk)
    return json.loads(buf)


def _recv_exact(
    sock: socket.socket, n: int, out_file: BinaryIO | None = None
) -> bytes | None:
    """Receive exactly n bytes. If out_file is given, write directly to it."""
    if out_file:
        received = 0
        while received < n:
            chunk = sock.recv(min(n - received, CHUNK_SIZE))
            if not chunk:
                return None
            out_file.write(chunk)
            received += len(chunk)
        return b""
    buf = bytearray(n)
    pos = 0
    while pos < n:
        chunk = sock.recv(min(n - pos, CHUNK_SIZE))
        if not chunk:
            return None
        buf[pos : pos + len(chunk)] = chunk
        pos += len(chunk)
    return bytes(buf)


def _send_file_data(sock: socket.socket, path: str) -> None:
    """Send raw file bytes over the socket."""
    with open(path, "rb") as f:
        while chunk := f.read(CHUNK_SIZE):
            sock.sendall(chunk)


def _get_all_ips() -> list[str]:
    """Get ALL local IPv4 addresses (all adapters: LAN, Wi-Fi, VPN, etc)."""
    ips: set[str] = set()

    # Method 1: hostname resolution
    try:
        for addr in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET):
            ip = addr[4][0]
            if ip not in ("127.0.0.1", "0.0.0.0"):
                ips.add(ip)
    except Exception:
        pass

    # Method 2: default route
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            s.connect(("8.8.8.8", 80))
            ip = s.getsockname()[0]
            if ip != "0.0.0.0":
                ips.add(ip)
    except Exception:
        pass

    # Method 3: Windows ipconfig
    if sys.platform == "win32":
        try:
            result = subprocess.run(
                ["ipconfig"],
                capture_output=True,
                text=True,
                creationflags=_NO_WINDOW,
                timeout=5,
            )
            for match in re.finditer(
                r"IPv4[^:]*:\s*(\d+\.\d+\.\d+\.\d+)", result.stdout
            ):
                ip = match.group(1)
                if ip != "127.0.0.1":
                    ips.add(ip)
        except Exception:
            pass

    return list(ips) if ips else ["127.0.0.1"]


def _get_display_ip() -> str:
    """Get the primary LAN IP (for display only)."""
    ips = _get_all_ips()
    for ip in ips:
        if ip.startswith("192.168."):
            return ip
    for ip in ips:
        if ip.startswith("10.") and not ip.startswith("10.15"):
            return ip
    for ip in ips:
        if ip.startswith("172."):
            return ip
    return ips[0] if ips else "127.0.0.1"


class LANPeer:
    """LAN peer with persistent TCP connection for instant transfers."""

    def __init__(
        self,
        code: str,
        on_event: Callable[[str, dict], None],
        on_log: Callable[[str, str], None],
    ):
        self._code = code
        self._code_hash = _hash_code(code)
        self._on_event = on_event
        self._on_log = on_log
        self._out_folder = ""

        self._running = False
        self._peer_ip: str | None = None
        self._conn: socket.socket | None = None
        self._conn_lock = threading.Lock()
        self._send_lock = threading.Lock()
        self._connected = False
        self._connect_time = 0.0

        self._beacon_thread: threading.Thread | None = None
        self._listen_thread: threading.Thread | None = None
        self._recv_thread: threading.Thread | None = None
        self._server_sock: socket.socket | None = None
        self._beacon_sock: socket.socket | None = None

    @property
    def connected(self) -> bool:
        return self._connected

    @property
    def peer_ip(self) -> str | None:
        return self._peer_ip

    def start(self, out_folder: str = "") -> None:
        """Start peer discovery and listening."""
        if self._running:
            return
        self._running = True
        self._out_folder = out_folder
        self._connected = False

        _ensure_firewall_rules()

        all_ips = _get_all_ips()
        display_ip = _get_display_ip()

        self._listen_thread = threading.Thread(
            target=self._tcp_listen, name="lan-tcp-listen", daemon=True
        )
        self._listen_thread.start()

        self._beacon_thread = threading.Thread(
            target=self._beacon_loop, name="lan-beacon", daemon=True
        )
        self._beacon_thread.start()

        if len(all_ips) > 1:
            self._on_log(
                "info", f"LAN direct: searching for peer (IPs: {', '.join(all_ips)})"
            )
        else:
            self._on_log("info", f"LAN direct: searching for peer (IP: {display_ip})")

    def stop(self) -> None:
        """Stop everything."""
        self._running = False
        with self._conn_lock:
            self._connected = False
            self._peer_ip = None
            conn = self._conn
            self._conn = None
        for s in (conn, self._server_sock, self._beacon_sock):
            if s:
                try:
                    s.close()
                except Exception:
                    pass
        self._server_sock = None
        self._beacon_sock = None

    def send_text(self, text: str) -> bool:
        """Send a text message instantly over the LAN connection."""
        with self._send_lock:
            with self._conn_lock:
                if not self._conn or not self._connected:
                    return False
                conn = self._conn
            try:
                _send_msg(conn, {"type": "text", "text": text})
                return True
            except Exception as e:
                logger.error("LAN send_text failed: %s", e)
                self._handle_disconnect(conn)
                return False

    def send_files(self, paths: list[str]) -> bool:
        """Send files/folders over the LAN connection."""
        file_list: list[tuple[str, str]] = []
        dir_list: list[str] = []

        for path in paths:
            path = os.path.normpath(path)
            if os.path.isdir(path):
                base = os.path.basename(path)
                dir_list.append(base)
                for root, dirs, files in os.walk(path):
                    rel_root = os.path.relpath(root, os.path.dirname(path))
                    for d in dirs:
                        dir_list.append(os.path.join(rel_root, d))
                    for f in files:
                        file_list.append(
                            (
                                os.path.join(root, f),
                                os.path.join(rel_root, f),
                            )
                        )
            elif os.path.isfile(path):
                file_list.append((path, os.path.basename(path)))
            else:
                logger.warning("LAN send: path not found: %s", path)

        if not file_list:
            logger.warning("LAN send: no files to send")
            return False

        with self._send_lock:
            with self._conn_lock:
                if not self._conn or not self._connected:
                    self._on_log(
                        "error",
                        f"LAN direct: not connected (conn={'yes' if self._conn else 'no'}, "
                        f"connected={self._connected})",
                    )
                    return False
                conn = self._conn

            try:
                logger.info(
                    "LAN send: %d files, %d dirs", len(file_list), len(dir_list)
                )
                _send_msg(
                    conn, {"type": "batch", "count": len(file_list) + len(dir_list)}
                )

                for d in dir_list:
                    _send_msg(conn, {"type": "dir", "name": d})

                for full_path, rel_name in file_list:
                    size = os.path.getsize(full_path)
                    _send_msg(conn, {"type": "file", "name": rel_name, "size": size})
                    _send_file_data(conn, full_path)

                _send_msg(conn, {"type": "done"})
                return True

            except Exception as e:
                logger.error("LAN send_files failed: %s (%s)", e, type(e).__name__)
                self._on_log("error", f"LAN direct: transfer error — {e}")
                self._handle_disconnect(conn)
                return False

    # ── UDP Beacon ──

    def _beacon_loop(self) -> None:
        """Broadcast our presence and listen for peers."""
        for port in (BEACON_PORT, BEACON_PORT + 1):
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                sock.settimeout(BEACON_INTERVAL)
                sock.bind(("", port))
                self._beacon_sock = sock
                break
            except OSError as e:
                logger.error("Beacon bind port %d failed: %s", port, e)
                try:
                    sock.close()
                except Exception:
                    pass
        else:
            logger.error("Beacon totally failed")
            return

        beacon_data = BEACON_MAGIC + self._code_hash.encode("ascii")
        my_ips = set(_get_all_ips())
        last_broadcast = 0.0

        broadcast_addrs: set[str] = {"255.255.255.255"}
        for ip in my_ips:
            parts = ip.split(".")
            if len(parts) == 4:
                broadcast_addrs.add(f"{parts[0]}.{parts[1]}.{parts[2]}.255")

        logger.info("Beacon: my IPs=%s, broadcasting to %s", my_ips, broadcast_addrs)

        while self._running:
            try:
                now = time.monotonic()
                if now - last_broadcast >= BEACON_INTERVAL:
                    for bcast in broadcast_addrs:
                        try:
                            self._beacon_sock.sendto(beacon_data, (bcast, BEACON_PORT))
                        except OSError:
                            pass
                    last_broadcast = now

                try:
                    data, addr = self._beacon_sock.recvfrom(256)
                except TimeoutError:
                    continue
                except OSError:
                    if not self._running:
                        break
                    continue

                if len(data) < len(BEACON_MAGIC) + 16:
                    continue
                if not data.startswith(BEACON_MAGIC):
                    continue

                peer_hash = data[len(BEACON_MAGIC) :].decode("ascii", errors="ignore")
                peer_ip = addr[0]

                if peer_ip in my_ips:
                    continue

                if peer_hash == self._code_hash and not self._connected:
                    self._on_log(
                        "info", f"LAN direct: found peer at {peer_ip}, connecting..."
                    )
                    self._try_connect(peer_ip)
                elif peer_hash != self._code_hash:
                    logger.debug("Beacon from %s: hash mismatch", peer_ip)

            except Exception as e:
                if self._running:
                    logger.error("Beacon loop error: %s", e)
                    time.sleep(1)

    # ── TCP Server ──

    def _tcp_listen(self) -> None:
        """Accept incoming TCP connections from peers."""
        try:
            self._server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self._server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self._server_sock.settimeout(2.0)
            self._server_sock.bind(("0.0.0.0", TRANSFER_PORT))
            self._server_sock.listen(1)
        except OSError as e:
            self._on_log(
                "error", f"LAN direct: TCP port {TRANSFER_PORT} unavailable: {e}"
            )
            return

        while self._running:
            try:
                conn, addr = self._server_sock.accept()
            except TimeoutError:
                continue
            except OSError:
                if not self._running:
                    break
                continue

            try:
                conn.settimeout(CONNECT_TIMEOUT)
                msg = _recv_msg(conn)
                if (
                    msg
                    and msg.get("type") == "auth"
                    and msg.get("hash") == self._code_hash
                ):
                    _send_msg(conn, {"type": "auth_ok"})
                    conn.settimeout(None)
                    self._establish_connection(conn, addr[0], inbound=True)
                else:
                    conn.close()
            except Exception:
                try:
                    conn.close()
                except Exception:
                    pass

    def _try_connect(self, peer_ip: str) -> None:
        """Initiate TCP connection to a discovered peer."""
        if self._connected:
            return
        if time.monotonic() - self._connect_time < 3.0:
            return
        sock = None
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(CONNECT_TIMEOUT)
            sock.connect((peer_ip, TRANSFER_PORT))

            _send_msg(sock, {"type": "auth", "hash": self._code_hash})
            msg = _recv_msg(sock)
            if msg and msg.get("type") == "auth_ok":
                sock.settimeout(None)
                self._establish_connection(sock, peer_ip)
                sock = None  # ownership transferred
            else:
                sock.close()
                sock = None
        except Exception as e:
            self._on_log("warn", f"LAN direct: connection to {peer_ip} failed: {e}")
            if sock:
                try:
                    sock.close()
                except Exception:
                    pass

    def _establish_connection(
        self, conn: socket.socket, peer_ip: str, *, inbound: bool = False
    ) -> None:
        """Set up a connected peer — start receive thread.

        Deterministic tie-breaking (à la libp2p): when both peers connect
        simultaneously, the peer with the **lower IP** keeps its outbound
        connection and the peer with the **higher IP** keeps its inbound
        connection.  Both sides agree, so exactly one TCP socket survives.
        """
        with self._conn_lock:
            if self._connected:
                # Already have a connection — apply tie-breaking
                my_ip = _get_display_ip()
                i_am_lower = my_ip < peer_ip
                # Lower IP keeps outbound, higher IP keeps inbound
                should_keep_new = (i_am_lower and not inbound) or (
                    not i_am_lower and inbound
                )
                if should_keep_new:
                    # Replace the existing connection with this better one
                    old_conn = self._conn
                    if old_conn:
                        try:
                            old_conn.close()
                        except Exception:
                            pass
                    logger.info(
                        "LAN: replacing connection (tie-break: my_ip=%s peer=%s "
                        "inbound=%s)",
                        my_ip,
                        peer_ip,
                        inbound,
                    )
                else:
                    # Discard this connection, keep existing
                    try:
                        conn.close()
                    except Exception:
                        pass
                    return

            conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            try:
                conn.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1024 * 1024)
                conn.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024 * 1024)
            except OSError:
                pass
            try:
                conn.setsockopt(socket.SOL_SOCKET, socket.SO_KEEPALIVE, 1)
            except OSError:
                pass

            old_conn = self._conn
            if old_conn:
                try:
                    old_conn.close()
                except Exception:
                    pass

            self._conn = conn
            self._peer_ip = peer_ip
            self._connected = True
            self._connect_time = time.monotonic()

        self._on_event("lan_connected", {"peer_ip": peer_ip})
        self._on_log("success", f"LAN direct: connected to {peer_ip}")

        self._recv_thread = threading.Thread(
            target=self._recv_loop, args=(conn,), name="lan-recv", daemon=True
        )
        self._recv_thread.start()

        threading.Thread(target=self._ping_loop, name="lan-ping", daemon=True).start()

    def _handle_disconnect(self, conn: socket.socket | None = None) -> None:
        """Handle peer disconnection.

        If *conn* is given, only disconnect when it is still the active
        connection.  Stale recv-loops pass their own socket so they
        never tear down a newer, valid connection.
        """
        with self._conn_lock:
            # Stale connection — ignore entirely
            if conn is not None and self._conn is not conn:
                return

            was_connected = self._connected
            self._connected = False
            self._peer_ip = None
            old_conn = self._conn
            self._conn = None

        if old_conn:
            try:
                old_conn.close()
            except Exception:
                pass

        if was_connected:
            self._on_event("lan_disconnected", {})
            self._on_log("info", "LAN direct: peer disconnected")

    # ── Receive Loop ──

    def _recv_loop(self, conn: socket.socket) -> None:
        """Handle incoming messages on the persistent connection."""
        # Bail immediately if this connection is already superseded
        with self._conn_lock:
            if self._conn is not conn:
                return
        while self._running and self._connected:
            try:
                msg = _recv_msg(conn)
                if msg is None:
                    break

                match msg.get("type"):
                    case "text":
                        self._on_event(
                            "lan_text_received", {"text": msg.get("text", "")}
                        )
                    case "ping":
                        with self._conn_lock:
                            if self._conn is conn:
                                _send_msg(conn, {"type": "pong"})
                    case "pong":
                        pass
                    case "batch":
                        self._recv_batch(conn, msg.get("count", 0))
                    case "file":
                        self._recv_file(conn, msg)

            except Exception as e:
                if self._running and self._connected:
                    logger.error("LAN recv error: %s", e)
                break

        self._handle_disconnect(conn)

    def _recv_file(self, conn: socket.socket, header: dict) -> None:
        """Receive a single file from the connection."""
        name = header.get("name", "unnamed")
        size = header.get("size", 0)

        out_dir = self._out_folder or os.path.expanduser("~")
        out_path = os.path.normpath(os.path.join(out_dir, name))

        if not out_path.startswith(os.path.normpath(out_dir)):
            logger.error("Path traversal attempt: %s", name)
            _recv_exact(conn, size)
            return

        os.makedirs(os.path.dirname(out_path), exist_ok=True)

        with open(out_path, "wb") as f:
            result = _recv_exact(conn, size, out_file=f)

        if result is not None:
            logger.info("Received file: %s (%d bytes)", name, size)
        else:
            logger.error("Incomplete file: %s", name)

    def _recv_batch(self, conn: socket.socket, count: int) -> None:
        """Receive a batch of files/directories."""
        received_files: list[str] = []

        while True:
            msg = _recv_msg(conn)
            if msg is None:
                break

            match msg.get("type"):
                case "done":
                    break
                case "dir":
                    dir_name = msg.get("name", "")
                    out_dir = self._out_folder or os.path.expanduser("~")
                    dir_path = os.path.normpath(os.path.join(out_dir, dir_name))
                    if dir_path.startswith(os.path.normpath(out_dir)):
                        os.makedirs(dir_path, exist_ok=True)
                case "file":
                    self._recv_file(conn, msg)
                    received_files.append(os.path.basename(msg.get("name", "unnamed")))

        if received_files:
            self._on_event("lan_files_received", {"files": received_files})

    def _ping_loop(self) -> None:
        """Send periodic pings to detect dead connections."""
        while self._running and self._connected:
            time.sleep(PING_INTERVAL)
            with self._conn_lock:
                if not self._conn or not self._connected:
                    break
                conn = self._conn
            try:
                _send_msg(conn, {"type": "ping"})
            except Exception:
                self._handle_disconnect(conn)
                break
