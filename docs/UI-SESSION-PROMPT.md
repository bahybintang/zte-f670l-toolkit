# Next-session prompt — build an on-device admin UI for the ZTE F670L (clone)

Paste everything below into a fresh **Opus 5.5** Claude Code session, run from
`~/Documents/Personal/Repositories/zte-f670l-fix`. This is a **development** task
(building software + reading/writing the router's config DB on a **spare** router);
there is no firmware flashing and no destructive on-device work.

---

You are building a **new router admin web UI** that replaces **all** functionality of
the stock ZTE F670L admin page, served **from the router itself** as a **single
lightweight binary** written in a **memory-safe language**, with a **beautiful
shadcn-style UI**. The stock UI's 5 GHz Wi-Fi save is broken (a prior session proved
why and built the workaround this UI will productize). This is my own device, a spare
on a separate internet link — development, fully authorized.

## FIRST: grill me, then build unattended

Start by loading the **`grilling`** skill and interrogate me (one question at a time,
with your recommended answer each time) until we have a shared understanding. Look up
*facts* yourself (filesystem, device, toolchain); put *decisions* to me. Cover at least:

1. **Scope of "all features."** The stock admin exposes dozens of pages (Status/device
   info, WAN/Internet, LAN, Wi-Fi 2.4/5 GHz incl. guest/WPS/MAC-ACL, DHCP server +
   static leases, IPv6, static routes, port forwarding / DMZ / port triggering,
   firewall, DDNS, UPnP, parental controls / time rules, QoS, USB/storage/Samba/DLNA,
   diagnostics (ping/traceroute), admin (users/password/reboot/backup-restore), TR-069,
   VoIP if present). Decide: truly 100% parity up front, or phased (Wi-Fi first — already
   proven — then WAN/LAN/DHCP/firewall/port-forward, then the long tail)? Recommended:
   **phased, but architect for 100% from day one** (generic DB-view ⇄ form engine so new
   pages are config, not code).
2. **Language / runtime.** Recommended: **Rust**, static binary, no GC, tiny RAM. (Go is
   the fallback — single binary too, but larger/GC.) Decide Rust vs Go.
3. **Cross-compile target.** Device is `armv7l`, glibc, Linux 4.1.25. Recommended target
   **`armv7-unknown-linux-musleabihf`** (fully *static* musl → runs regardless of the
   device's glibc version; no `ld-linux`/glibc-version risk). VERIFY the float ABI first
   (`readelf -A` on a device binary for `Tag_ABI_VFP_args`); fall back to
   `…musleabi` (soft-float) if needed. Decide: static-musl (recommended) vs glibc-gnueabihf.
4. **Config access from the binary.** Recommended: **shell out to the device's own
   `sidbg 1 DB …` and `iwpriv`** (proven, universal, covers every feature) rather than
   reimplementing the DB/OSS protocol. Decide. (Option B: reverse the DB format for direct
   reads — faster but large effort; keep as a later optimization.)
5. **Serving model.** Run as a daemon on a **non-80 port** (e.g. 8088) alongside the stock
   `httpd` (so nothing breaks), or replace stock. Recommended: **coexist on :8088**, started
   at boot. Decide the port + whether to also proxy/keep stock.
6. **Startup/persistence.** Where the binary lives and how it auto-starts across reboots
   (writable jffs2: `/opt/upt/apps` has space; hook via the init path). Decide.
7. **Auth.** The UI writes config — it must authenticate. Recommended: login with the
   existing `admin` credential (or a app-specific password), signed-cookie session,
   bound to LAN only. Decide auth model + whether to bind to LAN interface only.
8. **Frontend delivery.** shadcn/ui (React + Tailwind + Radix) built to static assets,
   **embedded into the Rust binary** (`rust-embed`) so it's truly single-file. Decide.
9. **Apply semantics.** Reads via `sidbg 1 DB p <Tbl>`; writes via `sidbg 1 DB set … ;
   sidbg 1 DB save`; Wi-Fi live via `iwpriv` (no reboot). Some features may need an
   apply/commit step beyond `save` — decide per-area how "apply" maps, and whether a
   global "apply & persist" vs per-field.
10. **Risk posture.** It's a spare router, but mistakes can misconfigure WAN/firewall.
    Decide: a confirm step for WAN/firewall/reboot writes; an auto-backup of
    `db_user_cfg.xml` before each write; a "revert last change" button.

Once answered, **develop end-to-end unattended**: set up the toolchain, build backend +
frontend, cross-compile, deploy to the router over tftp, run on-device, and test real
reads/writes (start with Wi-Fi, which is already proven). Iterate until it runs on the
router and configures it. Capture findings in the `zte-f670l` okf bundle as you go.

## Verified device facts (from the prior session — trust these)

- **Hardware = genuine F670L platform** (confirmed by diffing a real F670L V9 image):
  SoC **ZX279128S**, kernel **Linux 4.1.25 ARMv7 (`armv7l`)**, **glibc** (`libc.so.6`,
  `/lib/ld-linux.so.3`), **512 MB RAM**, U-Boot V8.3.0, dual-image A/B
  (`kernel0/1`, `rootfs0/1`; booting `rootfs0`=mtd8). GPON GN25L95. Wi-Fi **MT7603 (2.4 GHz)
  + MT7663 (5 GHz)**, MediaTek RTPRIV driver.
- **Wi-Fi interface map:** 2.4 GHz = `wlan0`–`wlan3` = WLAN1–4 = DB rows 0–3;
  5 GHz = `wlan4`–`wlan7` = WLAN5–8 = DB rows 4–7.
- **Access:** router `192.168.1.1`; this Mac has alias `192.168.1.100` on the link
  (re-add if gone: `sudo ifconfig en0 alias 192.168.1.100 255.255.255.0`). Root **telnet
  `root` / `Pon521`** — drive with `python3 -m zte670 telnet "<cmd>"`. Stock web `admin`/`admin`.
- **Filesystem:** `/` is read-only jffs2 (`/dev/mtdblock8`). Writable jffs2 mounts:
  `/userconfig` (mtd5, the config store), `/opt/upt/apps` (mtd12, ~30 MB free — put the
  binary here), `/tagparam` (mtd2), `/wlan` (mtd7). `dd` is absent; move files with `cat`
  or tftp. Binary transfer: `python3 tools/tftpd.py --dir <d> --port 6969 &` on the Mac,
  then on device `tftp -g -l <local> -r <remote> 192.168.1.100 6969` (Mac→device).
- **`sidbg` absent in PATH? No — it's present.** `sendcmd` is NOT present; the DB tool is
  **`sidbg 1 DB`**. Subcommands: `all`, `p <Tbl>` (print table XML), `pv` (list all view
  schemas: ID/Dsc/parent/len/max), `set <Tbl> <row> <DM> <val>`, `save` (persist running
  DB → `/userconfig/cfg`), `addr <Tbl>` (add row), `delr <Tbl> <row>`, `pti <Tbl>` (detail),
  `decry <path>`. `sidbg 1 -p` prints the module/PCB table. `set` echoes the whole table on
  success — redirect to /dev/null and confirm with a read-back.

## Proven config mechanisms (this is the whole engine)

```sh
# READ any config area (returns <Tbl name=..><Row No=N><DM name=.. val=../>…):
sidbg 1 DB p WLANCfg          # Wi-Fi SSIDs/enable/hide/alias (rows 0-7)
sidbg 1 DB p WLANPSK          # Wi-Fi passwords (KeyPassphrase, rows 0-7; read back as ******)
sidbg 1 DB pv                 # the full list of config views/tables = the feature map

# WRITE + PERSIST any field (no -258; verified — bypasses the broken stock apply path):
sidbg 1 DB set WLANCfg 4 ESSID 'MyName' >/dev/null 2>&1
sidbg 1 DB save              >/dev/null 2>&1

# LIVE Wi-Fi apply, NO REBOOT (MediaTek iwpriv — verified instant on wlan4):
iwpriv wlan4 set SSID='MyName'
iwpriv wlan4 set HideSSID=1
iwpriv wlan4 set WPAPSK='secret123'
```

The 5 GHz web bug (context): the stock UI's 5 GHz apply synchronously notifies OSS module
`0x0001014B` ("NMG") that this build never registers → `iRet -258`, nothing written. Even
genuine F670L firmware has no such module — it applies 5 GHz **inline via these same
`iwpriv` driver calls**. So **`sidbg set/save` + `iwpriv` IS the correct mechanism**; the
new UI uses it directly and sidesteps the stock bug. The whole admin surface is just DB
tables (readable/writable via `sidbg`) + a few live-apply hooks (Wi-Fi via `iwpriv`;
most other areas take effect via `save` + the normal running-config path, which works for
everything except the 5 GHz stock-apply quirk — verify per-area).

## Resources already in the repo

- **Working prototype (the mechanism reference):** `tools/webui/` — a Python-stdlib
  backend + single-file HTML panel that already configures all 8 WLANs (SSID/password/
  enable/hide) **no-restart** via `iwpriv`+`sidbg`, tested end-to-end. Port it to the Rust
  on-device design; reuse the exact command recipes.
- **CLI tools:** `tools/set_wifi_live.py` (no-restart), `tools/apply_wifi.py` (file-edit+reboot).
- **Toolkit:** `python3 -m zte670 telnet "<cmd>"`, `tools/tftpd.py`. `zte670/device.py` has
  the telnet helper (user `root`, pass `Pon521`) and the web session class.
- **Knowledge bundle (read first):** `~/Documents/Personal/Knowledge/zte-f670l/` (okf CLI,
  not MCP). `okf search "<q>" <bundle>` + `okf show <id> <bundle>`. The `log.md` has the full
  history; `issues/wlan5-web-set-258.md` the root cause. Capture new findings there
  (`okf validate --strict`, then it auto-syncs per the knowledge rule).
- **Reference genuine-firmware rootfs** (for mapping stock pages → DB tables, and copying
  option lists/validation): `debug/ref-fw-v9/extracted/` (gitignored). The stock UI pages
  live in `home/httpd/` there — a goldmine for exact field↔DM mappings and enums.
- Build host is this Mac (Apple Silicon). Rust via rustup; add the ARM target; or use
  `cross`/`zig cc`. **Verify a hello-world cross-build runs on the device before building big.**

## Build / deploy / test loop

1. Cross-compile the binary on the Mac → tftp to `/opt/upt/apps/adminui/` on the device.
2. Run it there (daemon on :8088); hit it from the Mac browser at `http://192.168.1.1:8088`.
3. Test a real round-trip: read WLANCfg, change a 5 GHz SSID, confirm live (`iwconfig wlan4`)
   + persisted (`sidbg 1 DB p WLANCfg`), revert. Expand area by area.
4. Add a boot hook so it survives reboots once stable.

## Guardrails (development, but still a live router)

- **Auto-backup `/userconfig/cfg/db_user_cfg.xml` before any write** (pull a copy to the
  Mac and/or `cp` on device) so every change is revertible; add a "revert" path.
- **Keep the stock `httpd` running** (coexist on a different port) so there's always a
  fallback UI and nothing on the device is removed.
- Be careful with **WAN / firewall / reboot** writes — gate them behind an explicit confirm.
- Everything the UI does is config-level (`sidbg`/`iwpriv`) — **no binary patching, no
  firmware flashing, no touching the bootloader/partitions.** All reversible.
- Device is a **spare** (not providing internet) — fine to reboot/experiment, but still
  back up config and keep changes revertible.

Deliver: a single cross-compiled binary that runs on the router, serves a beautiful
shadcn-style admin UI on the LAN, authenticates, and reads/writes the real config
(Wi-Fi working no-restart first, then broadening toward full parity), plus a short
README (build + deploy + run) and updated okf knowledge.
