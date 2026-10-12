# zte-f670l-fix

Tools and findings for a store-bought router labelled **ZTE F670L** that is actually a
**YYKF clone** (real board: ZTE **F628S**, MediaTek **MT7663** dual-band). Its 5 GHz Wi-Fi
works but **cannot be edited from the web UI** — every 5 GHz save fails with
`Web set failed (OBJ_WLANCONFIG_ID identity: IGD.LD1.WLAN5 iRet: -258)`.

**TL;DR:** the web 5 GHz page is broken by a firmware bug (see below). You *can* change 5 GHz
reliably with the `zte670` toolkit here, which edits the config directly and bypasses the broken
page. Fixing the web page itself needs firmware-level work — see [RE handoff](docs/RE-SESSION-PROMPT.md).

## Device

| | |
|---|---|
| Label / real board | F670L (clone) / ZTE **F628S**, SoC ZXIC, MediaTek MT7663 dual-band, EPON |
| Address | `192.168.1.1` |
| Web admin | `admin` / `admin` |
| Root telnet | `root` / `Pon521` (port 23; the password is stored in the config, `TelnetUser`) |
| Radios | 2.4 GHz = `wlan0`, 5 GHz = `wlan4` (4 VAPs/band: wlan0–3, wlan4–7) |

## The 5 GHz bug (root cause, proven)

A 5 GHz config save is routed by the web daemon to an internal message handler that **this
firmware build never registered**:

```
webd (OSS addr 0x00011701) --event 0xae09--> module 0x0001014B   ← not registered → iRet -258
```

Registered WLAN handlers are `wlan_config` (`0x00010301`) and `wlan_mgr` (`0x00011201`);
`0x0001014B` has no owner. 2.4 GHz saves route to a registered handler and work. It is **not**
the data (rates/channel are valid), the web JavaScript, or the interface name — it is a missing
handler in the compiled firmware. A factory reset cannot fix it (firmware is unchanged), and no
replacement firmware exists online. The clean fixes are: **use the config route** (below), or get
the **seller/manufacturer to reflash** the build where that handler is registered.

Full investigation: the `zte-f670l` bundle in the personal knowledge repo.

## Toolkit (`zte670`)

```bash
python3 -m venv .venv && . .venv/bin/activate
pip install -r requirements.txt
```

```bash
python3 -m zte670 unpack  ctce8_F670L.cfg config.xml     # decode a USB backup → plaintext DB XML
python3 -m zte670 wifi    config.xml                     # list every SSID / PSK
python3 -m zte670 set-wifi config.xml 5 --ssid NAME --psk PASS -o new.xml   # edit 5 GHz (inst 5)
python3 -m zte670 pack    new.xml out.cfg -t ctce8_F670L.cfg                # repack USB backup
python3 -m zte670 unpack-db db_user_cfg.xml live.xml     # decode the live on-device DB file
python3 -m zte670 pack-db   live.new.xml db_user_cfg.new -t db_user_cfg.xml # byte-exact repack
python3 -m zte670 telnet  "sendcmd ..."                  # run a root telnet command (WRITES)
```

WLAN instances: **1–4 = 2.4 GHz, 5–8 = 5 GHz**. Passwords are in `WLANPSK`
(`IGD.LD1.WLAN<n>.PSK1` → `KeyPassphrase`).

## Changing 5 GHz the working way (config route)

The live config is a file on the device: `/userconfig/cfg/db_user_cfg.xml` (a version-0
"ecb-zero" DB container — AES-128-ECB with an all-zero key; the device does not checksum it, so a
toolkit repack is accepted byte-for-byte). Pull it, edit, push back, reboot — this never touches
the broken web handler:

```bash
python3 tools/tftpd.py --dir backup/device --port 6969 &          # TFTP server on 192.168.1.100
python3 -m zte670 telnet "cd /userconfig/cfg && tftp -p -l db_user_cfg.xml -r db_user_cfg.xml 192.168.1.100 6969"
python3 -m zte670 unpack-db backup/device/db_user_cfg.xml live.xml
python3 -m zte670 set-wifi live.xml 5 --ssid "MyWifi-5G" --psk "secret123" -o live.new.xml
python3 -m zte670 pack-db live.new.xml backup/device/db_user_cfg.new -t backup/device/db_user_cfg.xml
python3 -m zte670 telnet \
  "cd /userconfig/cfg && tftp -g -l db_user_cfg.xml -r db_user_cfg.new 192.168.1.100 6969" \
  "cd /userconfig/cfg && tftp -g -l db_backup_cfg.xml -r db_user_cfg.new 192.168.1.100 6969" \
  "reboot"
```

Revert: TFTP the saved original `db_user_cfg.xml` back and reboot. Same-length SSID/PSK edits keep
the file byte-exact; a length-changing edit uses `pack` + the web USB-restore page instead.

## Backups (git-ignored, in `backup/`)

| Path | What |
|---|---|
| `backup/firmware/mtd0_wholeflash.bin.gz` | Whole flash, 256 MiB → 26 MB gz, md5 `7dc74ca9…` (complete recovery image; also on the USB stick) |
| `backup/device/db_user_cfg.xml`, `db_backup_cfg.xml`, `db_default_cfg.xml` | Live, backup, and factory-default configs |
| `backup/device/paramtag`, `mtd2.bin` | Device keys / SN / MAC |
| `backup/bin/cspd`, `backup/bin/httpd` | The two firmware binaries for the RE work (stripped ARM) |

These hold passwords and firmware, so `backup/` is git-ignored.

## Repo layout

```
zte670/            the CLI toolkit (cfg decode/edit/repack, web + telnet access)
tools/tftpd.py     minimal TFTP server for binary transfer to/from the device
backup/            git-ignored device data + firmware + RE binaries
debug/             git-ignored local-only investigation material (tool repos, decoded configs); see debug/README.md
docs/              RE handoff prompt
requirements.txt   pycryptodomex + zte-cfgs
```

## Status / next steps

- Working: diagnosis complete, config-route 5 GHz editing, full backup.
- Open: fixing the **web** 5 GHz page needs firmware binary RE (register/re-route module
  `0x0001014B`). Handoff and full context: **[docs/RE-SESSION-PROMPT.md](docs/RE-SESSION-PROMPT.md)**.

## Credits

- [zte-cfgs](https://github.com/OpticalNetworkTerminal/zte-cfgs) — ctce8 / ZTE DB decode-encode.
- [zteOnu](https://github.com/Septrum101/zteOnu) — ZTE factory telnet (does **not** work on this
  clone; its `webFac` endpoints are 404).
