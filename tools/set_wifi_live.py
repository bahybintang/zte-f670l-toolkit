#!/usr/bin/env python3
"""
set_wifi_live.py — change a WLAN instance (incl. 5 GHz) via the device's own DB
tool, the clean way that works on this router.

Why this exists
---------------
The web UI cannot save 5 GHz: a live config-apply routes to an OSS management
module (0x0001014B, "NMG") that this firmware build never registers, so it
returns iRet -258 and writes nothing. BUT the device's own debug DB interface
writes the config WITHOUT that broken apply path:

    sidbg 1 DB set WLANCfg <row> ESSID <value>   # writes running DB, no -258 (verified)
    sidbg 1 DB save                              # persists running DB -> config file
    reboot                                       # boot-init applies it to the radio

This tool does exactly that and verifies the result. It needs NO binary patch,
touches no telnet/cspd binaries, and is fully reversible.

Row map (WLANCfg table): WLAN1..4 (2.4 GHz) = rows 0..3, WLAN5..8 (5 GHz) = rows 4..7.

Usage
-----
  python3 tools/set_wifi_live.py 5 --ssid "Home-5G"            # WLAN5 (5 GHz) SSID
  python3 tools/set_wifi_live.py 5 --ssid "Home-5G" --psk "secret12"
  python3 tools/set_wifi_live.py 5 --ssid "Home-5G" --no-reboot   # set+save, apply later
  python3 tools/set_wifi_live.py --show                         # print current WLAN SSIDs
"""
from __future__ import annotations

import argparse
import sys
import time
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
sys.path.insert(0, str(REPO))
from zte670 import device  # noqa: E402

# sidbg set uses the DM name; PSK lives in a sibling table keyed per instance.
ESSID_DM = "ESSID"


def _t(ip: str, pw: str, *cmds: str) -> str:
    return device.telnet_run(list(cmds), ip=ip, password=pw)


def _wait_up(ip: str, timeout: int = 240) -> bool:
    end = time.time() + timeout
    while time.time() < end:
        if subprocess.run(["ping", "-c1", "-t2", ip], capture_output=True).returncode == 0 \
           and subprocess.run(["nc", "-z", "-G2", ip, "23"], capture_output=True).returncode == 0:
            return True
        time.sleep(4)
    return False


def show(ip: str, pw: str) -> int:
    out = _t(ip, pw, "sidbg 1 DB p WLANCfg 2>/dev/null | grep -E 'Row No|ESSID|SSIDAlias'")
    print(out)
    return 0


def set_wifi(inst: int, ssid: str | None, psk: str | None, ip: str, pw: str,
             reboot: bool) -> int:
    if not (1 <= inst <= 8):
        print("instance must be 1-8"); return 2
    row = inst - 1  # WLAN<n> -> row n-1
    cmds = []
    if ssid is not None:
        cmds.append(f"sidbg 1 DB set WLANCfg {row} {ESSID_DM} '{ssid}'")
    if psk is not None:
        # PSK table: WLANPSK, keyed to the same instance; field KeyPassphrase.
        cmds.append(f"sidbg 1 DB set WLANPSK {row} KeyPassphrase '{psk}'")
    if not cmds:
        print("give --ssid and/or --psk"); return 2
    cmds.append("sidbg 1 DB save")   # persist running DB -> /userconfig/cfg
    cmds.append("echo SET_DONE")

    print(f"[1/3] writing WLAN{inst} (row {row}) via sidbg DB set + save ...")
    out = _t(ip, pw, *cmds)
    if "SET_DONE" not in out or "-258" in out:
        print("ERROR: set/save did not complete cleanly:\n" + out[-600:]); return 3
    print("      written to running DB and saved to config file.")

    if not reboot:
        print("[--no-reboot] config saved; reboot the router to apply 5 GHz changes.")
        return 0

    print("[2/3] rebooting to apply (boot-init configures the radio) ...")
    _t(ip, pw, "reboot")
    if not _wait_up(ip):
        print("WARNING: device didn't come back in time; check manually."); return 4
    time.sleep(5)

    print("[3/3] verifying ...")
    iface = "wlan0" if inst <= 4 else "wlan4"
    chk = _t(ip, pw, f"iwconfig {iface} 2>/dev/null | grep ESSID")
    print("      " + chk.strip().splitlines()[-1] if chk.strip() else "      (no ESSID line)")
    if ssid is not None and ssid in chk:
        print(f"      SUCCESS: {iface} now broadcasts {ssid!r}")
    elif ssid is not None:
        print(f"      NOTE: expected {ssid!r}; if different, the instance maps to another VAP.")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("instance", nargs="?", type=int, help="WLAN 1-8 (5-8 = 5 GHz)")
    ap.add_argument("--ssid")
    ap.add_argument("--psk")
    ap.add_argument("--no-reboot", action="store_true")
    ap.add_argument("--show", action="store_true", help="print current WLAN SSIDs")
    ap.add_argument("--ip", default=device.DEFAULT_IP)
    ap.add_argument("--password", default="Pon521")
    a = ap.parse_args(argv)
    if a.show:
        return show(a.ip, a.password)
    if a.instance is None:
        ap.error("give an instance (1-8) with --ssid/--psk, or --show")
    return set_wifi(a.instance, a.ssid, a.psk, a.ip, a.password, not a.no_reboot)


if __name__ == "__main__":
    raise SystemExit(main())
