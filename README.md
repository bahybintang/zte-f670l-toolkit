# zte-f670l-toolkit

A replacement admin web UI and a reverse-engineering toolkit for the **"ZTE F670L" clone
ONT**: a fiber router sold under the F670L label, built by YYKF on a ZTE (ZXIC) chipset.

The stock web UI on these units cannot save any 5 GHz Wi-Fi setting. This project fixes that
and adds features the stock firmware hides. **adminui** is a modern admin UI that runs on the
router itself, next to the stock one. The **`zte670`** Python toolkit reads and edits the
router's configuration from your computer.

> [!WARNING]
> This is unofficial software for your own hardware. It runs as root on the router and edits
> its configuration database. No firmware image for this clone exists online, so **back up the
> whole flash first** (see [Safety and recovery](#safety-and-recovery)). Use it at your own risk.

## Contents

- [Is this my router?](#is-this-my-router)
- [What works](#what-works)
- [Quick start](#quick-start)
- [Safety and recovery](#safety-and-recovery)
- [Documentation](#documentation)
- [Repository layout](#repository-layout)
- [Contributing](#contributing)
- [Credits](#credits)

## Is this my router?

Probably, if **all** of these match:

| Check | Expected |
|---|---|
| Label | ZTE **F670L**, PON fiber router (ONT), 4 LAN ports, dual-band Wi-Fi |
| Stock web UI | `http://192.168.1.1`, page footer or banner `Mini web server 1.0 YYKF corp 2024` |
| Status page | Hardware `V9.0`, software `V9.0.0P10T12`, serial number starting with `YYKF` (genuine ZTE units start with `ZTEG`) |
| The bug | Saving any 5 GHz setting fails with `Web set failed (OBJ_WLANCONFIG_ID identity: IGD.LD1.WLAN5 iRet: -258)` |

Inside, it is not the ZTE board the label suggests:

| Part | What it is |
|---|---|
| SoC | ZTE/ZXIC **ZX279128** (ARM Cortex-A9, no FPU), Linux 4.1.25 |
| Wi-Fi | MediaTek **MT7603E** (2.4 GHz, `wlan0`–`wlan3`) and **MT7613** (5 GHz, `wlan4`–`wlan7`), on PCIe |
| Flash | 256 MiB, config in a JFFS2 partition (`/userconfig`) |
| Access | Web on port 80; root telnet on port 23 on the LAN |

Genuine ZTE tools (for example `zteOnu` and its factory endpoints) do **not** work on this clone,
and genuine ZTE firmware must never be flashed onto it. See [docs/hardware.md](docs/hardware.md).

## What works

### Available

**adminui: the on-device admin UI** ([adminui/README.md](adminui/README.md))

- **Every stock page, rebuilt.** It covers all 125 stock pages (status, network, Wi-Fi, security, applications, admin and diagnostics), including the 5 GHz settings the stock UI cannot save. Pages that don't apply to this hardware are hidden.
- **Changes apply live.** 46 of the 77 writable pages take effect immediately, the way the stock UI applies them. The rest are saved and take effect at the next restart, and each page says which kind it is.
- **Firewall rules apply immediately.** That covers IP and MAC filters, service control, DMZ and port forwarding.
- **Safe changes.** Every value is validated and read back, and a failed write rolls back. The config is snapshotted before each change, every change can be undone from the history, and WAN, LAN and firewall changes are commit-confirm: they revert after 3 minutes unless you keep them.
- **Network overview.**
  - A live map of the router, its LAN ports, Wi-Fi bands and connected devices.
  - A devices list.
  - A QR code for joining Wi-Fi.
- **Tools.**
  - Ping and traceroute.
  - A config-database browser.
  - Flash partition download, read-only.
- **Hardened.**
  - It listens on the LAN only.
  - It uses an Argon2id login, HttpOnly cookies and CSRF protection.
  - It refuses changes that would lock you out of telnet unless you confirm them.

**EasyMesh (MediaTek Multi-AP)**, which the firmware ships but never starts ([adminui/notes/easymesh.md](adminui/notes/easymesh.md))

- **Controller.** Manages EasyMesh agents connected by Ethernet.
- **Agent over Ethernet.** Tested with a TP-Link Archer AX12 as the controller: the router onboards and takes the controller's Wi-Fi settings.
- **Wi-Fi uplink.** The router joins another router's 5 GHz network and bridges it to its LAN and Wi-Fi, so it works as a wireless extender. The mesh map shows the link and its signal.

**`zte670` toolkit** ([docs/toolkit.md](docs/toolkit.md))

- Decodes, edits and re-encodes the USB backup (`.cfg`) and the live config database file.
- Edits any Wi-Fi network, 5 GHz included, through the config file, the route that bypasses the broken stock handler.
- Includes a telnet helper and a small TFTP server for moving files to and from the router.

### Work in progress

| Item | State |
|---|---|
| EasyMesh agent **onboarding over Wi-Fi** | The Wi-Fi uplink works, but the router is not yet listed as a satellite by the controller. WPS completes but returns the controller's normal Wi-Fi credentials instead of its backhaul network. Findings are in [easymesh.md](adminui/notes/easymesh.md#onboarding-over-wi-fi-work-in-progress). |
| WAN connect, disconnect and renew buttons | Built; not yet validated on a unit with a live fiber link ([#9](../../issues/9)). |
| Restart-only pages | WAN IPoE, the LAN bridge and most VoIP pages have no live-apply path in the firmware, so they need a restart. |
| Loop-detection page | Saves the setting, but the kernel's loop detector does not read it. |

### Not possible

- **Fixing the stock 5 GHz page.** The firmware sends the 5 GHz save to an internal handler that this build never registers. It is a compiled-in defect, not bad data, so a factory reset does not help. Use adminui or the toolkit instead. Details: [docs/5ghz-bug.md](docs/5ghz-bug.md).
- **OpenWrt.** The ZX279128 SoC has no OpenWrt or mainline Linux support. Only the Wi-Fi chips are supported upstream.

## Quick start

### Install adminui on the router

You need a computer on the router's LAN with a fixed address the router can reach for TFTP
(the default is `192.168.1.100`).

```bash
# one-time build setup (macOS shown; any host with these tools works)
brew install rustup zig node
rustup default stable && rustup target add armv7-unknown-linux-musleabi
cargo install cargo-zigbuild
(cd adminui/web && npm install)

# build the web app and the server, copy it to the router over TFTP, install and start it
adminui/scripts/deploy.sh                     # or: --router 192.168.1.1 --host <your-ip>
```

Then open **`http://192.168.1.1:8088`** and sign in with `admin` / `admin`. adminui asks you
to choose a new password. The stock UI stays on port 80 as a fallback. adminui starts again at
every boot.

### Use the toolkit

```bash
python3 -m venv .venv && . .venv/bin/activate
pip install -r requirements.txt
python3 -m zte670 unpack  backup.cfg config.xml        # decode a USB config backup
python3 -m zte670 wifi    config.xml                   # list every SSID and its security
```

Full command list and the config-file route for changing 5 GHz: [docs/toolkit.md](docs/toolkit.md).

## Safety and recovery

- **Back up the whole flash before writing anything.** With a USB stick in the router, run
  over telnet: `cat /dev/mtdblock0 > /mnt/usb1_1/mtd0_wholeflash.bin`, then compare `md5sum` on
  both sides. The router has no `dd`.
- **Keep telnet enabled.** LAN telnet is the recovery path if the web UIs fail. adminui refuses
  to turn it off unless you explicitly confirm.
- **Turn adminui off:** `touch /userconfig/adminui.disable` over telnet. **Remove it:**
  `rm /etc/rcS.d/S99zadminui && rm -r /opt/upt/apps/adminui`.
- **Forgotten adminui password:** `/opt/upt/apps/adminui/adminui passwd`, then
  `/opt/upt/apps/adminui/adminui.sh restart`.
- **Never run the firmware's DB or `pm_*` library functions from another process.** Attaching to
  the config database's shared memory and crashing takes down the whole management daemon, and
  only a power cycle recovers.

Default access: web `admin` / `admin`. Root telnet is user `root`; its password is stored in the
config database (`TelnetUser` table), and on the units examined it was `Pon521`.

## Documentation

| Document | What's in it |
|---|---|
| [docs/hardware.md](docs/hardware.md) | Hardware, firmware layout, interfaces, access and file transfer |
| [docs/5ghz-bug.md](docs/5ghz-bug.md) | Root cause of the stock 5 GHz save failure |
| [docs/toolkit.md](docs/toolkit.md) | The `zte670` CLI, the config-file route and the helper tools |
| [adminui/README.md](adminui/README.md) | adminui: how it applies changes, build, deploy, on-router layout, security |
| [adminui/schema/SCHEMA.md](adminui/schema/SCHEMA.md) | The page schema that drives both the server and the web forms |
| [adminui/notes/oss-protocol.md](adminui/notes/oss-protocol.md) | The firmware's internal messaging (OSS) and how live apply works |
| [adminui/notes/easymesh.md](adminui/notes/easymesh.md) | EasyMesh controller and agent, Wi-Fi backhaul, firmware traits |
| [adminui/web/README.md](adminui/web/README.md) | Developing the web app |

## Repository layout

```
adminui/        on-device admin UI: Rust server, React web app, page schema, mesh shims, notes
zte670/         Python toolkit: config decode/edit/repack, web and telnet access
tools/          TFTP server, Wi-Fi helper scripts, a minimal local Wi-Fi web panel
docs/           hardware, the 5 GHz bug, toolkit guide; docs/agents/ holds AI-agent conventions
backup/ debug/  git-ignored: your device dumps and local investigation files (they hold passwords)
```

## Contributing

Issues and pull requests are welcome, especially from owners of other F670L clone builds.
When reporting, include the hardware and software versions from the Status page.

- **Server tests:** `cd adminui/server && cargo test`. A schema test fails if a page file
  doesn't parse or names a table the firmware's database doesn't have.
- **Web app:** `cd adminui/web && npm run dev` serves on `localhost:5173` and proxies `/api`
  to the router.
- **Never commit device data.** Configs, dumps and logs contain Wi-Fi and account passwords.
  `backup/`, `debug/`, `*.cfg` and `*.xml` are git-ignored for that reason.

## Credits

- [zte-cfgs](https://github.com/OpticalNetworkTerminal/zte-cfgs) for the ZTE config format.
- [zteOnu](https://github.com/Septrum101/zteOnu), useful background on ZTE ONTs, although its
  factory telnet does not work on this clone.
- MediaTek's EasyMesh userland, whose source is public in the ASUS GPL tree
  ([asuswrt-merlin.ng](https://github.com/RMerl/asuswrt-merlin.ng)).

## License

[MIT](LICENSE). Third-party components keep their own licenses: the MediaTek EasyMesh
binaries that adminui starts belong to the router's firmware and are not part of this
repository.
