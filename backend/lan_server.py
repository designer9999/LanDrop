"""
LAN Direct Transfer — instant file & text transfer over local network.

Architecture:
  - UDP broadcast beacon for automatic peer discovery (port 29170)
  - TCP server for persistent connections (port 29171)
  - Protocol: length-prefixed JSON headers + raw binary file data
  - Zero-copy sendfile() on Windows for maximum throughput

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
import socket
import struct
import threading
import time

logger = logging.getLogger(__name__)

BEACON_PORT = 29170
TRANSFER_PORT = 29171
BEACON_MAGIC = b"CRUDE1"
HEADER_FMT = "!I"  # 4-byte big-endian length prefix
CHUNK_SIZE = 256 * 1024  # 256KB read chunks
BEACON_INTERVAL = 2.0
PING_INTERVAL = 10.0
CONNECT_TIMEOUT = 5.0


def _hash_code(code: str) -> str:
    """Hash the code phrase for safe broadcast (don't expose raw code on network)."""
    return hashlib.sha256(code.encode("utf-8")).hexdigest()[:16]


def _send_msg(sock: socket.socket, data: dict) -> None:
    """Send a length-prefixed JSON message."""
    payload = json.dumps(data, separators=(",", ":")).encode("utf-8")
    sock.sendall(struct.pack(HEADER_FMT, len(payload)) + payload)


def _recv_msg(sock: socket.socket) -> dict | None:
    """Receive a length-prefixed JSON message. Returns None on disconnect."""
    header = b""
    while len(header) < 4:
        chunk = sock.recv(4 - len(header))
        if not chunk:
            return None
        header += chunk
    (length,) = struct.unpack(HEADER_FMT, header)
    if length > 100 * 1024 * 1024:  # 100MB max header — sanity check
        return None
    payload = b""
    while len(payload) < length:
        chunk = sock.recv(min(length - len(payload), CHUNK_SIZE))
        if not chunk:
            return None
        payload += chunk
    return json.loads(payload.decode("utf-8"))


def _recv_exact(sock: socket.socket, n: int, out_file=None) -> bytes | None:
    """Receive exactly n bytes. If out_file is given, write directly to it."""
    if out_file:
        received = 0
        while received < n:
            chunk = sock.recv(min(n - received, CHUNK_SIZE))
            if not chunk:
                return None
            out_file.write(chunk)
            received += len(chunk)
        return b""  # Signal success without holding data in memory
    else:
        data = b""
        while len(data) < n:
            chunk = sock.recv(min(n - len(data), CHUNK_SIZE))
            if not chunk:
                return None
            data += chunk
        return data


def _send_file(sock: socket.socket, path: str) -> bool:
    """Send a single file: header + raw bytes. Uses sendfile() if available."""
    try:
        size = os.path.getsize(path)
        name = os.path.basename(path)
        _send_msg(sock, {"type": "file", "name": name, "size": size})

        with open(path, "rb") as f:
            try:
                # Try zero-copy sendfile (Windows 3.12+, Linux, macOS)
                sock.sendfile(f)
            except (AttributeError, OSError):
                # Fallback: manual chunked send
                while True:
                    chunk = f.read(CHUNK_SIZE)
                    if not chunk:
                        break
                    sock.sendall(chunk)
        return True
    except Exception as e:
        logger.error("Failed to send file %s: %s", path, e)
        return False


class LANPeer:
    """Represents a connected LAN peer with persistent TCP connection."""

    def __init__(self, code: str, on_event, on_log):
        self._code = code
        self._code_hash = _hash_code(code)
        self._on_event = on_event  # callback(event_name, data_dict)
        self._on_log = on_log      # callback(level, msg)
        self._out_folder: str = ""

        # State
        self._running = False
        self._peer_ip: str | None = None
        self._conn: socket.socket | None = None
        self._conn_lock = threading.Lock()
        self._connected = False

        # Threads
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

        # Start TCP server
        self._listen_thread = threading.Thread(target=self._tcp_listen, daemon=True)
        self._listen_thread.start()

        # Start UDP beacon (broadcast + listen)
        self._beacon_thread = threading.Thread(target=self._beacon_loop, daemon=True)
        self._beacon_thread.start()

        self._on_log("info", f"LAN direct: listening (code hash {self._code_hash[:8]}...)")

    def stop(self) -> None:
        """Stop everything."""
        self._running = False
        self._connected = False
        self._peer_ip = None

        # Close sockets to unblock threads
        for s in (self._conn, self._server_sock, self._beacon_sock):
            if s:
                try:
                    s.close()
                except Exception:
                    pass
        self._conn = None
        self._server_sock = None
        self._beacon_sock = None

    def send_text(self, text: str) -> bool:
        """Send a text message instantly over the LAN connection."""
        with self._conn_lock:
            if not self._conn or not self._connected:
                return False
            try:
                _send_msg(self._conn, {"type": "text", "text": text})
                return True
            except Exception as e:
                logger.error("LAN send_text failed: %s", e)
                self._handle_disconnect()
                return False

    def send_files(self, paths: list[str]) -> bool:
        """Send files/folders over the LAN connection."""
        with self._conn_lock:
            if not self._conn or not self._connected:
                return False
            conn = self._conn

        try:
            # Collect all files (expand directories)
            file_list: list[tuple[str, str]] = []  # (full_path, relative_name)
            dir_list: list[str] = []  # relative dir paths to create

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
                            full = os.path.join(root, f)
                            rel = os.path.join(rel_root, f)
                            file_list.append((full, rel))
                elif os.path.isfile(path):
                    file_list.append((path, os.path.basename(path)))

            total_count = len(file_list) + len(dir_list)
            _send_msg(conn, {"type": "batch", "count": total_count})

            # Send directory entries first
            for d in dir_list:
                _send_msg(conn, {"type": "dir", "name": d})

            # Send files
            names_sent = []
            for full_path, rel_name in file_list:
                size = os.path.getsize(full_path)
                _send_msg(conn, {"type": "file", "name": rel_name, "size": size})

                with open(full_path, "rb") as f:
                    try:
                        conn.sendfile(f)
                    except (AttributeError, OSError):
                        while True:
                            chunk = f.read(CHUNK_SIZE)
                            if not chunk:
                                break
                            conn.sendall(chunk)

                names_sent.append(os.path.basename(rel_name))

            _send_msg(conn, {"type": "done"})
            return True

        except Exception as e:
            logger.error("LAN send_files failed: %s", e)
            self._handle_disconnect()
            return False

    # ── UDP Beacon ──

    def _beacon_loop(self) -> None:
        """Broadcast our presence and listen for peers."""
        try:
            self._beacon_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            self._beacon_sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
            self._beacon_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self._beacon_sock.settimeout(BEACON_INTERVAL)
            self._beacon_sock.bind(("", BEACON_PORT))
        except OSError as e:
            logger.error("Beacon bind failed: %s", e)
            # Try with a different port
            try:
                self._beacon_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                self._beacon_sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
                self._beacon_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                self._beacon_sock.settimeout(BEACON_INTERVAL)
                self._beacon_sock.bind(("", BEACON_PORT + 1))
            except OSError:
                logger.error("Beacon totally failed, giving up")
                return

        beacon_data = BEACON_MAGIC + self._code_hash.encode("ascii")
        my_ip = self._get_my_ip()
        last_broadcast = 0.0

        while self._running:
            try:
                # Broadcast our beacon
                now = time.monotonic()
                if now - last_broadcast >= BEACON_INTERVAL:
                    try:
                        self._beacon_sock.sendto(beacon_data, ("<broadcast>", BEACON_PORT))
                    except OSError:
                        pass
                    last_broadcast = now

                # Listen for peer beacons
                try:
                    data, addr = self._beacon_sock.recvfrom(256)
                except socket.timeout:
                    continue
                except OSError:
                    if not self._running:
                        break
                    continue

                if len(data) < len(BEACON_MAGIC) + 16:
                    continue
                if not data.startswith(BEACON_MAGIC):
                    continue

                peer_hash = data[len(BEACON_MAGIC):].decode("ascii", errors="ignore")
                peer_ip = addr[0]

                # Skip our own broadcasts
                if peer_ip == my_ip:
                    continue

                # Check if this peer has the same code
                if peer_hash == self._code_hash and not self._connected:
                    logger.info("Discovered LAN peer at %s", peer_ip)
                    self._try_connect(peer_ip)

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
            logger.error("TCP listen failed on port %d: %s", TRANSFER_PORT, e)
            return

        while self._running:
            try:
                conn, addr = self._server_sock.accept()
            except socket.timeout:
                continue
            except OSError:
                if not self._running:
                    break
                continue

            # Authenticate: peer sends code hash
            try:
                conn.settimeout(CONNECT_TIMEOUT)
                msg = _recv_msg(conn)
                if msg and msg.get("type") == "auth" and msg.get("hash") == self._code_hash:
                    # Send auth response
                    _send_msg(conn, {"type": "auth_ok"})
                    conn.settimeout(None)
                    self._establish_connection(conn, addr[0])
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
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(CONNECT_TIMEOUT)
            sock.connect((peer_ip, TRANSFER_PORT))

            # Authenticate
            _send_msg(sock, {"type": "auth", "hash": self._code_hash})
            msg = _recv_msg(sock)
            if msg and msg.get("type") == "auth_ok":
                sock.settimeout(None)
                self._establish_connection(sock, peer_ip)
            else:
                sock.close()
        except Exception as e:
            logger.debug("LAN connect to %s failed: %s", peer_ip, e)

    def _establish_connection(self, conn: socket.socket, peer_ip: str) -> None:
        """Set up a connected peer — start receive thread."""
        if self._connected:
            try:
                conn.close()
            except Exception:
                pass
            return

        # Optimize socket for throughput
        conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        try:
            # 1MB send/recv buffer for high throughput
            conn.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1024 * 1024)
            conn.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024 * 1024)
        except OSError:
            pass

        with self._conn_lock:
            self._conn = conn
            self._peer_ip = peer_ip
            self._connected = True

        self._on_event("lan_connected", {"peer_ip": peer_ip})
        self._on_log("success", f"LAN direct: connected to {peer_ip}")

        # Start receive thread
        self._recv_thread = threading.Thread(target=self._recv_loop, args=(conn,), daemon=True)
        self._recv_thread.start()

        # Start keepalive ping
        threading.Thread(target=self._ping_loop, daemon=True).start()

    def _handle_disconnect(self) -> None:
        """Handle peer disconnection."""
        was_connected = self._connected
        with self._conn_lock:
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
        while self._running and self._connected:
            try:
                msg = _recv_msg(conn)
                if msg is None:
                    break

                msg_type = msg.get("type")

                if msg_type == "text":
                    text = msg.get("text", "")
                    self._on_event("lan_text_received", {"text": text})

                elif msg_type == "ping":
                    with self._conn_lock:
                        if self._conn:
                            _send_msg(self._conn, {"type": "pong"})

                elif msg_type == "pong":
                    pass  # Keepalive ack

                elif msg_type == "batch":
                    self._recv_batch(conn, msg.get("count", 0))

                elif msg_type == "file":
                    self._recv_file(conn, msg)

            except Exception as e:
                if self._running and self._connected:
                    logger.error("LAN recv error: %s", e)
                break

        self._handle_disconnect()

    def _recv_file(self, conn: socket.socket, header: dict) -> None:
        """Receive a single file from the connection."""
        name = header.get("name", "unnamed")
        size = header.get("size", 0)

        out_dir = self._out_folder or os.path.expanduser("~")
        # Handle relative paths (subdirectories)
        out_path = os.path.normpath(os.path.join(out_dir, name))

        # Security: ensure the path stays within out_dir
        if not out_path.startswith(os.path.normpath(out_dir)):
            logger.error("Path traversal attempt: %s", name)
            # Still need to consume the bytes
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
        received_files = []

        while True:
            msg = _recv_msg(conn)
            if msg is None:
                break

            msg_type = msg.get("type")

            if msg_type == "done":
                break

            elif msg_type == "dir":
                dir_name = msg.get("name", "")
                out_dir = self._out_folder or os.path.expanduser("~")
                dir_path = os.path.normpath(os.path.join(out_dir, dir_name))
                if dir_path.startswith(os.path.normpath(out_dir)):
                    os.makedirs(dir_path, exist_ok=True)

            elif msg_type == "file":
                self._recv_file(conn, msg)
                name = msg.get("name", "unnamed")
                received_files.append(os.path.basename(name))

        if received_files:
            self._on_event("lan_files_received", {"files": received_files})

    def _ping_loop(self) -> None:
        """Send periodic pings to detect dead connections."""
        while self._running and self._connected:
            time.sleep(PING_INTERVAL)
            with self._conn_lock:
                if not self._conn or not self._connected:
                    break
                try:
                    _send_msg(self._conn, {"type": "ping"})
                except Exception:
                    self._handle_disconnect()
                    break

    @staticmethod
    def _get_my_ip() -> str:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
                s.connect(("8.8.8.8", 80))
                return s.getsockname()[0]
        except Exception:
            return "127.0.0.1"
