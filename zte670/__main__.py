"""zte670 — toolkit for the store-bought YYKF "F670L" clone. See README.md."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import cfg, device


def _p(obj):
    print(json.dumps(obj, indent=2, ensure_ascii=False))


def cmd_unpack(a):
    _, xml = cfg.unpack(Path(a.cfg))
    Path(a.out).write_bytes(xml)
    print(f"wrote {a.out} ({len(xml)} bytes)")


def cmd_pack(a):
    xml = Path(a.xml).read_bytes()
    info = cfg.pack(xml, Path(a.template), Path(a.out), model=a.model)
    _p(info)


def cmd_wifi(a):
    xml = Path(a.xml).read_text("utf-8", "replace")
    _p(cfg.show_wifi(xml))


def cmd_setwifi(a):
    xml = Path(a.xml).read_text("utf-8", "replace")
    xml = cfg.set_wifi(xml, a.instance, ssid=a.ssid, psk=a.psk)
    out = a.out or a.xml
    Path(out).write_text(xml, "utf-8")
    print(f"updated WLAN{a.instance} -> {out}")


def cmd_unpack_db(a):
    xml = cfg.unpack_container(Path(a.db))
    Path(a.out).write_bytes(xml)
    print(f"wrote {a.out} ({len(xml)} bytes)")


def cmd_pack_db(a):
    xml = Path(a.xml).read_bytes()
    template = Path(a.template).read_bytes()
    out = cfg.pack_container(xml, template)
    Path(a.out).write_bytes(out)
    identical = out == template
    print(f"wrote {a.out} ({len(out)} bytes)"
          + (" — byte-identical to template" if identical else ""))


def cmd_telnet(a):
    print(device.telnet_run(a.command, ip=a.ip, password=a.password))


def main(argv=None):
    ap = argparse.ArgumentParser(prog="zte670", description=__doc__)
    ap.add_argument("--ip", default=device.DEFAULT_IP)
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("unpack", help="ctce8 .cfg -> plaintext XML")
    s.add_argument("cfg"); s.add_argument("out")
    s.set_defaults(func=cmd_unpack)

    s = sub.add_parser("pack", help="plaintext XML -> ctce8 .cfg (clone format)")
    s.add_argument("xml"); s.add_argument("out")
    s.add_argument("-t", "--template", required=True, help="original .cfg")
    s.add_argument("--model", default="F670L")
    s.set_defaults(func=cmd_pack)

    s = sub.add_parser("wifi", help="list SSID/PSK for all WLAN instances")
    s.add_argument("xml")
    s.set_defaults(func=cmd_wifi)

    s = sub.add_parser("set-wifi", help="edit one WLAN instance's SSID/PSK in XML")
    s.add_argument("xml"); s.add_argument("instance", type=int)
    s.add_argument("--ssid"); s.add_argument("--psk"); s.add_argument("-o", "--out")
    s.set_defaults(func=cmd_setwifi)

    s = sub.add_parser("unpack-db", help="raw DB container (db_user_cfg.xml) -> XML")
    s.add_argument("db"); s.add_argument("out")
    s.set_defaults(func=cmd_unpack_db)

    s = sub.add_parser("pack-db", help="XML -> device-exact DB container (same-length edits)")
    s.add_argument("xml"); s.add_argument("out")
    s.add_argument("-t", "--template", required=True, help="original db_user_cfg.xml")
    s.set_defaults(func=cmd_pack_db)

    s = sub.add_parser("telnet", help="run command(s) over root telnet (WRITES)")
    s.add_argument("command", nargs="+")
    s.add_argument("--password", default="Pon521")
    s.set_defaults(func=cmd_telnet)

    a = ap.parse_args(argv)
    try:
        a.func(a)
    except Exception as e:  # noqa: BLE001 - CLI boundary
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
