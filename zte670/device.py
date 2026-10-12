"""
Live access to the router: web UI (admin) and root telnet.

Web session is IP-based once logged in; writes need the per-page _SESSION_TOKEN.
Telnet users root/user share one password stored in the config (TelnetUser).
"""
from __future__ import annotations

import re
import socket
import time
import urllib.parse
import urllib.request

DEFAULT_IP = "192.168.1.1"


# --- Web -------------------------------------------------------------------
class Web:
    def __init__(self, ip: str = DEFAULT_IP, user: str = "admin", password: str = "admin"):
        self.base = f"http://{ip}"
        self.user, self.password = user, password
        self._jar = urllib.request.HTTPCookieProcessor()
        self._opener = urllib.request.build_opener(self._jar)

    def _post(self, path: str, fields: dict) -> str:
        data = urllib.parse.urlencode(fields).encode()
        req = urllib.request.Request(self.base + path, data=data,
                                     headers={"Referer": self.base + "/"})
        with self._opener.open(req, timeout=10) as r:
            return r.read().decode("utf-8", "replace")

    def login(self) -> None:
        self._post("/", {"action": "login", "Username": self.user,
                         "Password": self.password, "Frm_Logintoken": "1"})

    def page(self, nextpage: str, **fields) -> str:
        q = {"IF_ACTION": "", **fields}
        return self._post(f"/getpage.gch?pid=1002&nextpage={nextpage}", q)

    @staticmethod
    def token(html: str) -> str | None:
        m = re.search(r'var session_token = "(\d+)"', html)
        return m.group(1) if m else None

    @staticmethod
    def value(html: str, name: str) -> str | None:
        hits = re.findall(rf"Transfer_meaning\('{re.escape(name)}','([^']*)'\)", html)
        return hits[-1].replace("\\x2e", ".").replace("\\x2d", "-").replace(
            "\\x2c", ",") if hits else None


# --- Telnet ----------------------------------------------------------------
_IAC, _DO, _DONT, _WILL, _WONT = 0xFF, 0xFD, 0xFE, 0xFB, 0xFC


def _negotiate(sock: socket.socket, data: bytes) -> bytes:
    """Refuse every telnet option; return the remaining data bytes."""
    out, i = bytearray(), 0
    while i < len(data):
        b = data[i]
        if b == _IAC and i + 2 < len(data):
            cmd, opt = data[i + 1], data[i + 2]
            if cmd in (_DO, _DONT):
                sock.sendall(bytes([_IAC, _WONT, opt]))
            elif cmd in (_WILL, _WONT):
                sock.sendall(bytes([_IAC, _DONT, opt]))
            i += 3
            continue
        out.append(b)
        i += 1
    return bytes(out)


def _read_until(sock: socket.socket, timeout: float = 3.0) -> bytes:
    sock.settimeout(timeout)
    buf = b""
    try:
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            buf += _negotiate(sock, chunk)
            if buf.rstrip().endswith((b":", b"#", b"$", b">")):
                break
    except socket.timeout:
        pass
    return buf


def telnet_run(commands: list[str], ip: str = DEFAULT_IP,
               user: str = "root", password: str = "Pon521",
               port: int = 23, timeout: float = 4.0) -> str:
    """Log in and run commands sequentially; return the full transcript.

    This writes to the device — only run commands you intend to apply.
    """
    s = socket.create_connection((ip, port), timeout=5)
    transcript = bytearray()
    try:
        transcript += _read_until(s, timeout)            # Login:
        s.sendall(user.encode() + b"\n"); time.sleep(0.4)
        transcript += _read_until(s, timeout)            # Password:
        s.sendall(password.encode() + b"\n"); time.sleep(0.6)
        transcript += _read_until(s, timeout)            # shell prompt
        for cmd in commands:
            s.sendall(cmd.encode() + b"\n"); time.sleep(0.6)
            transcript += _read_until(s, timeout)
        s.sendall(b"exit\n")
    finally:
        s.close()
    return transcript.decode("utf-8", "replace")
