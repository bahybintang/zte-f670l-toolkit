# Hardware, firmware and access

What is known about the YYKF "ZTE F670L" clone, from its Status page, a root shell and the
firmware binaries. Values marked *(observed)* came from one unit; other batches may differ.
Please open an issue if yours does.

## Identification

| Field | Value |
|---|---|
| Label | ZTE F670L, PON ONT |
| Hardware / software version | `V9.0` / `V9.0.0P10T12` *(observed)* |
| Manufacturer | YYKF. Serial numbers start with `YYKF`; genuine ZTE starts with `ZTEG` |
| Web server banner | `Mini web server 1.0 YYKF corp 2024` |
| PON | EPON |

## Inside

| Part | Detail |
|---|---|
| SoC | ZTE/ZXIC **ZX279128**, ARM Cortex-A9 without VFP/NEON, so binaries must be **soft-float** (`armv7-unknown-linux-musleabi`) |
| Kernel | Linux 4.1.25, Buildroot 2017.05 userland, glibc, loader `/lib/ld-linux.so.3` |
| 2.4 GHz Wi-Fi | MediaTek **MT7603E**, driver `mt7603e_ap.ko`, interfaces `wlan0`–`wlan3`, station `apcli0` |
| 5 GHz Wi-Fi | MediaTek **MT7613**, driver `mtk7613e_ap.ko`, interfaces `wlan4`–`wlan7`, station `apcli1` |
| Ethernet | 4 LAN ports behind an internal switch (`eth0`–`eth3`), bridged as `br0` |
| Flash | 256 MiB (`/dev/mtdblock0` is the whole device) |

There is **no OpenWrt or mainline Linux support** for the ZX279128 SoC. The two Wi-Fi chips are
supported by OpenWrt's `mt76` driver, but that does not help without a port for the SoC.

## Firmware layout that matters

| Path | What it is |
|---|---|
| `/userconfig/cfg/db_user_cfg.xml` | Live config database. Encrypted with AES-128-ECB and an all-zero key; no checksum. `db_backup_cfg.xml` is a mirror and `db_default_cfg.xml` holds the factory defaults |
| `/etc/db_tbl_def.xml` | Schema of every config table and field |
| `/etc` | Overlay whose upper directory (`/opt/upt/apps/etc`) persists, so `/etc/rcS.d/` hooks survive reboots |
| `/opt/upt/apps/bin/` | Wiped at every boot; don't install anything there |
| `/wlan` | Persistent JFFS2: Wi-Fi calibration and the EasyMesh SDK templates |
| `/var` | tmpfs |

Main processes: `cspd` (configuration and management daemon, holds the DB), `httpd` (stock web
UI), `sidbg` (the firmware's own debug and DB tool).

- `sidbg 1 DB set …` writes a value.
- `sidbg 1 DB save` persists it, but returns **before** the file is written (about 1 s later).
- `sidbg` exits 0 even on failure, so read every write back.

## Access

| | |
|---|---|
| Web UI | `http://192.168.1.1`, `admin` / `admin` |
| Root telnet | Port 23 on the LAN, user `root`. The password is in the config DB, table `TelnetUser`; it was `Pon521` on the units examined. ZTE defaults such as `Zte521` are rejected |
| Missing tools | `dd`, `sendcmd`, `ztedbcli`, `strace`, `gdb` |
| Present tools | `cat`, `tftp` (client), `md5sum`, `od`, `hexdump`, busybox |

### Moving files

Use **TFTP**. Pasting base64 over telnet corrupts binaries and mangles long lines. Run the
bundled server on your computer, then from the router's shell:

```bash
python3 tools/tftpd.py --dir ./transfer --port 6969     # on your computer
tftp -p -l <file> -r <name> <your-ip> 6969               # router → computer
tftp -g -l <file> -r <name> <your-ip> 6969               # computer → router
```

### Backing up

No firmware image for this clone is published, so make your own before changing anything:

```bash
# with a USB stick inserted in the router
cat /dev/mtdblock0 > /mnt/usb1_1/mtd0_wholeflash.bin && md5sum /mnt/usb1_1/mtd0_wholeflash.bin
```

Also copy the config files (`db_user_cfg.xml`, `db_backup_cfg.xml`, `db_default_cfg.xml`) and
`paramtag` (device keys, serial and MAC) to your computer over TFTP. They contain passwords, so
keep them out of any public repository.

## Things to avoid

- **Flashing genuine ZTE firmware.** The board and software are different.
- **Turning off LAN telnet in the config.** It survives a reboot and removes the recovery path.
- **Raw `mtd` writes.** Editing the config file is revertible; raw flash writes are not.
- **Attaching to the config DB's shared memory from your own process** (or calling the firmware's
  `pm_*` / DB library functions). If that process crashes, `cspd` dies with it and only a power
  cycle recovers.
