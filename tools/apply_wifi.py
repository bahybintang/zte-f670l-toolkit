#!/usr/bin/env python3
"""
apply_wifi.py — change a WLAN instance (incl. 5 GHz) the way that actually works
on this router: edit the stored config and reboot so cspd's boot-init applies it.

Why this exists
---------------
The web UI cannot save 5 GHz settings: a live config-apply routes to an OSS
management module (0x0001014B, "NMG") that this firmware build never registers,
so it returns iRet -258 and writes nothing. That live-apply path is the ONLY
thing broken — boot-init reads the config DB and configures every radio directly
(verified: a config edit + reboot changes wlan4's SSID). So the reliable way to
change 5 GHz is: edit the stored DB, push it, reboot.

This tool does the proven cycle end to end and verifies the result:
  pull live db_user_cfg.xml  ->  edit  ->  byte-exact repack  ->  push  ->  reboot  ->  verify

Scope / safety
--------------
- SAME-LENGTH edits only (byte-exact repack). A different-length SSID/PSK cannot
  be spliced byte-exact; use the ctce8 pack + web USB-restore path instead (see
  the config-edit runbook). The tool refuses rather than push a wrong-length file.
- The original pulled config is saved next to the work dir as *.orig for revert.
- Only db_user_cfg.xml is written; db_backup_cfg.xml is left as an on-device
  fallback. The USB flash image remains the ultimate recovery.
- Nothing here can touch telnet/cspd binaries; it only edits config data.

Usage
-----
  python3 tools/apply_wifi.py <instance> --ssid "NewName" [--psk "secret"]
  python3 tools/apply_wifi.py 5 --ssid "Home-5G" --psk "pass1234"   # 5 GHz = 5..8
  python3 tools/apply_wifi.py 5 --ssid "Home-5G" --dry-run          # prepare only
  python3 tools/apply_wifi.py --revert <saved .orig file>           # restore + reboot
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
sys.path.insert(0, str(REPO))

from zte670 import cfg, device  # noqa: E402

MAC_IP = "192.168.1.100"   # this host on the device link
TFTP_PORT = 6969
CFG_DIR = "/userconfig/cfg"
CFG_FILE = "db_user_cfg.xml"


def _tftpd(serve_dir: Path) -> subprocess.Popen:
    subprocess.run(["pkill", "-f", "tools/tftpd.py"], capture_output=True)
    time.sleep(1)
    p = subprocess.Popen(
        [sys.executable, str(HERE / "tftpd.py"), "--dir", str(serve_dir),
         "--port", str(TFTP_PORT)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1)
    return p


def _telnet(ip: str, pw: str, *cmds: str) -> str:
    return device.telnet_run(list(cmds), ip=ip, password=pw)


def _wait_up(ip: str, timeout: int = 240) -> bool:
    """Block until the device answers ping AND telnet, or timeout."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        ping = subprocess.run(["ping", "-c1", "-t2", ip], capture_output=True)
        if ping.returncode == 0:
            nc = subprocess.run(["nc", "-z", "-G2", ip, "23"], capture_output=True)
            if nc.returncode == 0:
                return True
        time.sleep(4)
    return False


def _live_ssid(ip: str, pw: str, iface: str) -> str:
    out = _telnet(ip, pw, f"iwconfig {iface} 2>/dev/null | grep ESSID")
    for line in out.splitlines():
        if "ESSID:" in line:
            return line.split("ESSID:", 1)[1].strip().strip('"')
    return "?"


def apply_edit(instance: int, ssid: str | None, psk: str | None,
               ip: str, pw: str, work: Path, dry_run: bool) -> int:
    work.mkdir(parents=True, exist_ok=True)
    serve = work  # tftp serves and receives here

    print(f"[1/6] pulling live {CFG_FILE} ...")
    _tftpd(serve)
    _telnet(ip, pw, f"cd {CFG_DIR} && tftp -p -l {CFG_FILE} -r cur.bin {MAC_IP} {TFTP_PORT}")
    cur = serve / "cur.bin"
    if not cur.exists() or cur.stat().st_size == 0:
        print("ERROR: pull failed (is the device reachable and the .100 alias up?)")
        return 2
    orig = work / "db_user_cfg.orig"
    orig.write_bytes(cur.read_bytes())
    print(f"      saved revert image: {orig} ({orig.stat().st_size} bytes)")

    print(f"[2/6] decoding + editing WLAN{instance} ...")
    plain = cfg.unpack_container(cur)
    edited = cfg.set_wifi(plain.decode("utf-8", "replace"), instance, ssid=ssid, psk=psk)

    print("[3/6] byte-exact repack ...")
    packed = cfg.pack_container(edited.encode("utf-8"), cur.read_bytes())
    if len(packed) != cur.stat().st_size:
        print(f"ERROR: length changed ({cur.stat().st_size} -> {len(packed)}). "
              "Byte-exact push is only for SAME-LENGTH edits. Use the ctce8 + "
              "USB-restore path for a different-length SSID/PSK. Nothing pushed.")
        return 3
    out = serve / "new.bin"
    out.write_bytes(packed)
    ndiff = sum(1 for a, b in zip(cur.read_bytes(), packed) if a != b)
    print(f"      OK: same length ({len(packed)} bytes), {ndiff} bytes differ "
          "(one AES block per changed field).")

    if dry_run:
        print("[dry-run] prepared only; not pushing or rebooting.")
        print(f"          test config: {out}")
        return 0

    print("[4/6] pushing new config to device (db_user_cfg.xml only) + reboot ...")
    _telnet(ip, pw,
            f"cd {CFG_DIR} && tftp -g -l {CFG_FILE} -r new.bin {MAC_IP} {TFTP_PORT} && md5sum {CFG_FILE}",
            "reboot")

    print("[5/6] waiting for reboot (up to 4 min) ...")
    if not _wait_up(ip):
        print("WARNING: device did not come back in time. If Wi-Fi/telnet are "
              f"down, restore with:  python3 tools/apply_wifi.py --revert {orig}")
        return 4
    time.sleep(5)

    print("[6/6] verifying ...")
    iface = "wlan0" if instance <= 4 else "wlan4"  # main VAP per band
    live = _live_ssid(ip, pw, iface)
    print(f"      live {iface} ESSID now: {live!r}")
    if ssid is not None:
        print("      SUCCESS" if live == ssid else
              f"      NOTE: expected {ssid!r}; if it differs the instance may map "
              "to a secondary VAP, or the device restored from db_backup_cfg.xml.")
    print(f"      revert any time with:  python3 tools/apply_wifi.py --revert {orig}")
    return 0


def revert(orig_path: Path, ip: str, pw: str) -> int:
    orig = Path(orig_path)
    if not orig.exists():
        print(f"ERROR: {orig} not found")
        return 2
    serve = orig.parent
    print(f"restoring {orig} -> device {CFG_FILE} + reboot ...")
    _tftpd(serve)
    _telnet(ip, pw,
            f"cd {CFG_DIR} && tftp -g -l {CFG_FILE} -r {orig.name} {MAC_IP} {TFTP_PORT} && md5sum {CFG_FILE}",
            "reboot")
    print("waiting for reboot ...")
    if not _wait_up(ip):
        print("WARNING: device not back in time; check manually.")
        return 4
    print("restored.")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("instance", nargs="?", type=int,
                    help="WLAN instance 1-8 (1-4 = 2.4 GHz, 5-8 = 5 GHz)")
    ap.add_argument("--ssid")
    ap.add_argument("--psk")
    ap.add_argument("--ip", default=device.DEFAULT_IP)
    ap.add_argument("--password", default="Pon521", help="root telnet password")
    ap.add_argument("--dry-run", action="store_true",
                    help="pull + edit + repack + validate only; no push/reboot")
    ap.add_argument("--revert", metavar="ORIG_FILE",
                    help="restore a saved db_user_cfg.orig and reboot")
    ap.add_argument("--work", default=str(REPO / "backup" / "device" / "apply_work"))
    a = ap.parse_args(argv)

    if a.revert:
        return revert(Path(a.revert), a.ip, a.password)
    if a.instance is None or (a.ssid is None and a.psk is None):
        ap.error("give an instance and at least one of --ssid/--psk (or --revert)")
    if not (1 <= a.instance <= 8):
        ap.error("instance must be 1-8")
    return apply_edit(a.instance, a.ssid, a.psk, a.ip, a.password,
                      Path(a.work), a.dry_run)


if __name__ == "__main__":
    raise SystemExit(main())
