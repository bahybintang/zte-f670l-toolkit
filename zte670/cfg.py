"""
ctce8 config decode / edit / encode for the store-bought "YYKF F670L" clone.

The USB backup (ctce8_<MODEL>.cfg) is a 128-byte wrapper (XOR-0x89 payload)
around a version-0 "ecb-zero" DB container: the plaintext <DB> XML is split
into 64 KiB chunks, each AES-128-ECB encrypted with an all-zero key. No
device-specific key is involved, so this works fully offline.

This module is a thin, documented wrapper over the `zte-cfgs` library
(https://github.com/OpticalNetworkTerminal/zte-cfgs), pinning the exact
round-trip that reproduces this clone's format (version 0, ecb-zero).
"""
from __future__ import annotations

import re
from pathlib import Path

from zte_cfgs import e8
from zte_cfgs.format import (pack_db, unpack_db, parse_header, parse_blocks,
                             encrypt_ecb_zero, DB_VARIANT_ECB_ZERO)


def unpack(cfg_path: Path) -> tuple[bytes, bytes]:
    """Return (inner_db_container, plaintext_xml) for a ctce8 .cfg."""
    inner_db, _meta = e8.extract_db(cfg_path)
    xml, _info = unpack_db(inner_db)
    return inner_db, xml


def unpack_container(db_path: Path) -> bytes:
    """Plaintext XML from a raw version-0 DB container (e.g. /userconfig/cfg/db_user_cfg.xml)."""
    xml, _ = unpack_db(db_path.read_bytes())
    return xml


def pack_container(xml: bytes, template: bytes) -> bytes:
    """Re-encode XML into a version-0 ecb-zero DB container, byte-exact to this
    clone's convention (CRC words zero, device 'next' values preserved) by
    splicing re-encrypted payloads into the template. Requires the edited XML to
    keep the template's block layout — true for same-length edits; raises
    otherwise so the caller can fall back to a full repack.
    """
    blocks = parse_blocks(template)
    if not blocks:
        raise ValueError("template has no blocks")
    split = blocks[0]["plain_len"]
    chunks = [xml[i:i + split] for i in range(0, len(xml), split)] or [b""]
    if len(chunks) != len(blocks):
        raise ValueError(
            f"block count changed ({len(chunks)} vs {len(blocks)}); "
            "length-changing edit — use ctce8 pack() + USB restore instead")
    out = bytearray(template)
    for chunk, b in zip(chunks, blocks):
        enc = encrypt_ecb_zero(chunk + b"\0" * (-len(chunk) % 16))
        if len(enc) != b["cipher_len"]:
            raise ValueError("cipher length changed; use full repack")
        out[b["payload_offset"]:b["payload_offset"] + b["cipher_len"]] = enc
    return bytes(out)


def pack(xml: bytes, template_cfg: Path, out_cfg: Path, model: str | None = None) -> dict:
    """Repack edited XML into a ctce8 .cfg, matching the template's format.

    Uses the template's *inner DB* as the pack template so the output is
    version-0 / ecb-zero exactly like the device-generated file.
    """
    inner_db, _ = e8.extract_db(template_cfg)
    tmp_db = out_cfg.with_suffix(".db.tmp")
    try:
        version = parse_header(inner_db)["version"]
        new_db = pack_db(xml, version, variant=DB_VARIANT_ECB_ZERO, template=inner_db)
        tmp_db.write_bytes(new_db)
        info = e8.repack_cfg(template_cfg, tmp_db, out_cfg, model=model)
    finally:
        tmp_db.unlink(missing_ok=True)
    return info


# --- XML helpers -----------------------------------------------------------
# The DB is a flat dump of <Tbl name=".."><Row No="n"><DM name=".." val=".."/>.
# WLAN1-4 are the 2.4 GHz SSIDs, WLAN5-8 are 5 GHz. Passwords live in WLANPSK
# as IGD.LD1.WLAN<n>.PSK1 / KeyPassphrase.

_SSID_INDEX = {  # friendly name -> WLAN instance number
    "2g": 1, "2.4g": 1, "wlan1": 1,
    "5g": 5, "wlan5": 5,
}


def _row_block(xml: str, view_name: str) -> tuple[int, int] | None:
    """Byte range of the <Row> whose ViewName DM equals view_name."""
    m = re.search(rf'<DM name="ViewName" val="{re.escape(view_name)}"/>', xml)
    if not m:
        return None
    start = xml.rfind("<Row", 0, m.start())
    end = xml.find("</Row>", m.start())
    return (start, end) if start != -1 and end != -1 else None


def get_dm(xml: str, view_name: str, dm: str) -> str | None:
    rng = _row_block(xml, view_name)
    if not rng:
        return None
    seg = xml[rng[0]:rng[1]]
    m = re.search(rf'<DM name="{re.escape(dm)}" val="([^"]*)"/>', seg)
    return m.group(1) if m else None


def set_dm(xml: str, view_name: str, dm: str, value: str) -> str:
    """Return xml with a single DM value replaced inside one Row. Raises if absent."""
    rng = _row_block(xml, view_name)
    if not rng:
        raise KeyError(f"Row {view_name!r} not found")
    seg = xml[rng[0]:rng[1]]
    new_seg, n = re.subn(
        rf'(<DM name="{re.escape(dm)}" val=")[^"]*("/>)',
        lambda mm: mm.group(1) + value + mm.group(2),
        seg,
    )
    if n != 1:
        raise KeyError(f"DM {dm!r} not found (exactly once) in {view_name!r}")
    return xml[:rng[0]] + new_seg + xml[rng[1]:]


def show_wifi(xml: str) -> list[dict]:
    """Summarise all 8 WLAN instances (SSID, PSK, enable)."""
    out = []
    for n in range(1, 9):
        base = f"IGD.LD1.WLAN{n}"
        essid = get_dm(xml, base, "ESSID")
        if essid is None:
            continue
        out.append({
            "instance": n,
            "band": "5G" if n >= 5 else "2.4G",
            "ssid": essid,
            "enable": get_dm(xml, base, "Enable"),
            "hidden": get_dm(xml, base, "ESSIDHideEnable"),
            "psk": get_dm(xml, f"{base}.PSK1", "KeyPassphrase"),
        })
    return out


def set_wifi(xml: str, instance: int, *, ssid: str | None = None,
             psk: str | None = None) -> str:
    """Set SSID and/or PSK for one WLAN instance (1-8)."""
    base = f"IGD.LD1.WLAN{instance}"
    if ssid is not None:
        xml = set_dm(xml, base, "ESSID", ssid)
    if psk is not None:
        xml = set_dm(xml, f"{base}.PSK1", "KeyPassphrase", psk)
    return xml
