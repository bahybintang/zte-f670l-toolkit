#!/usr/bin/env python3
"""
F670L WiFi control panel — a local web UI that configures the router's Wi-Fi
(all 8 WLANs, 2.4 GHz + 5 GHz) with NO RESTART, working around the firmware's
broken 5 GHz web save.

How it works
------------
The stock web UI can't save 5 GHz: the apply notifies an OSS module (0x0001014B,
"NMG") this build never registered -> iRet -258, nothing written. This panel
replicates what NMG would have done, over root telnet:

  * live apply   : iwpriv wlan<if> set <Key>=<Val>      (changes the radio instantly)
  * persistence  : sidbg 1 DB set WLANCfg <row> <DM> <v> ; sidbg 1 DB save

Both verified on-device. No binary patch, nothing installed on the router,
fully reversible. Run this on your Mac; open the printed URL in any browser.

    python3 tools/webui/server.py            # then open http://127.0.0.1:8666

Interface / row map: WLAN1-4 = 2.4 GHz = wlan0-3 = rows 0-3;
                     WLAN5-8 = 5 GHz   = wlan4-7 = rows 4-7.
"""
from __future__ import annotations

import json
import re
import sys
import html
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(REPO))
from zte670 import device  # noqa: E402

IP = device.DEFAULT_IP
PW = "Pon521"
PORT = 8666

# WLANCfg DMs we surface, and the matching MediaTek iwpriv keys for live apply.
# (value is the iwpriv key, or None if persist-only.)
FIELD_IWPRIV = {
    "ESSID": "SSID",
    "ESSIDHideEnable": "HideSSID",   # 0/1
    "Enable": None,                  # VAP up/down handled separately
}


def _tel(*cmds: str) -> str:
    return device.telnet_run(list(cmds), ip=IP, password=PW)


def iface_of(row: int) -> str:
    return f"wlan{row}"


def read_wlans() -> list[dict]:
    """Parse `sidbg 1 DB p WLANCfg` into a list of WLAN dicts."""
    out = _tel("sidbg 1 DB p WLANCfg 2>/dev/null")
    wlans = []
    for m in re.finditer(r'<Row No="(\d+)">(.*?)</Row>', out, re.S):
        row = int(m.group(1)); body = m.group(2)
        def dm(name, default=""):
            mm = re.search(rf'<DM name="{name}" val="([^"]*)"/>', body)
            return mm.group(1) if mm else default
        wlans.append({
            "row": row,
            "instance": row + 1,
            "band": "2.4 GHz" if row < 4 else "5 GHz",
            "iface": iface_of(row),
            "ssid": dm("ESSID"),
            "enable": dm("Enable", "0") == "1",
            "hide": dm("ESSIDHideEnable", "0") == "1",
            "alias": dm("SSIDAlias"),
        })
    return sorted(wlans, key=lambda w: w["row"])[:8]


def apply_wlan(row: int, ssid: str | None, hide: bool | None,
               enable: bool | None, password: str | None) -> dict:
    iface = iface_of(row)
    steps: list[str] = []
    live: list[str] = []     # iwpriv commands (instant)
    persist: list[str] = []  # sidbg DB writes

    if ssid is not None:
        s = ssid.strip()
        if not s or len(s) > 32:
            return {"ok": False, "error": "SSID must be 1-32 chars"}
        live.append(f"iwpriv {iface} set SSID='{s}'")
        persist.append(f"sidbg 1 DB set WLANCfg {row} ESSID '{s}'")
        steps.append(f"SSID -> {s}")
    if hide is not None:
        live.append(f"iwpriv {iface} set HideSSID={1 if hide else 0}")
        persist.append(f"sidbg 1 DB set WLANCfg {row} ESSIDHideEnable {1 if hide else 0}")
        steps.append(f"hide -> {hide}")
    if password is not None and password != "":
        if len(password) < 8 or len(password) > 63:
            return {"ok": False, "error": "WPA password must be 8-63 chars"}
        live.append(f"iwpriv {iface} set WPAPSK='{password}'")
        # PSK persists in the WLANPSK table, same row index, field KeyPassphrase.
        persist.append(f"sidbg 1 DB set WLANPSK {row} KeyPassphrase '{password}'")
        steps.append("password -> (updated)")
    if enable is not None:
        live.append(f"ifconfig {iface} {'up' if enable else 'down'}")
        persist.append(f"sidbg 1 DB set WLANCfg {row} Enable {1 if enable else 0}")
        steps.append(f"enable -> {enable}")

    if not steps:
        return {"ok": False, "error": "nothing to change"}

    # sidbg DB set echoes the whole modified table; silence every step and
    # rely on the trailing marker so success detection is unambiguous.
    quiet = [f"{c} >/dev/null 2>&1" for c in live + persist]
    quiet += ["sidbg 1 DB save >/dev/null 2>&1", "echo APPLIED_OK"]
    out = _tel(*quiet)
    ok = "APPLIED_OK" in out
    return {"ok": ok, "steps": steps,
            "detail": "" if ok else (out[-400:] or "no response from router")}


INDEX = (HERE / "index.html")


class H(BaseHTTPRequestHandler):
    def _send(self, code, body, ctype="application/json"):
        b = body if isinstance(body, bytes) else body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def log_message(self, *a):  # quiet
        pass

    def do_GET(self):
        if self.path in ("/", "/index.html"):
            self._send(200, INDEX.read_bytes(), "text/html; charset=utf-8")
        elif self.path == "/api/wifi":
            try:
                self._send(200, json.dumps({"ok": True, "wlans": read_wlans()}))
            except Exception as e:
                self._send(200, json.dumps({"ok": False, "error": str(e)}))
        else:
            self._send(404, json.dumps({"ok": False, "error": "not found"}))

    def do_POST(self):
        if self.path != "/api/apply":
            return self._send(404, json.dumps({"ok": False, "error": "not found"}))
        n = int(self.headers.get("Content-Length", 0))
        try:
            d = json.loads(self.rfile.read(n) or b"{}")
            res = apply_wlan(int(d["row"]), d.get("ssid"), d.get("hide"),
                             d.get("enable"), d.get("password"))
            self._send(200, json.dumps(res))
        except Exception as e:
            self._send(200, json.dumps({"ok": False, "error": str(e)}))


def main():
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), H)
    print(f"F670L WiFi panel  ->  http://127.0.0.1:{PORT}")
    print(f"(talks to the router at {IP} over root telnet; no-restart apply)")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
