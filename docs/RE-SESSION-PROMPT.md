# RE session prompt — fix the 5 GHz web-config bug on the YYKF "F670L" clone

Paste everything below into a fresh Claude Code session (run from
`~/Documents/Personal/Repositories/zte-f670l-fix`).

---

You are continuing a reverse-engineering effort on my own router (authorized; it's my device,
I accept the risk). Goal: make the **web UI able to save 5 GHz Wi-Fi settings** again. Load the
`reverse-engineer-anything` (REA) skill and work the binaries with it.

## The one-line bug (already proven)
The web daemon routes a 5 GHz config-apply to an internal OSS message handler that this firmware
build never registered, so the write fails:

```
webd (OSS addr 0x00011701) --event 0xAE09--> module 0x0001014B   ← NOT registered → iRet -258
```
- Registered WLAN handlers: `wlan_config` = `0x00010301`, `wlan_mgr` = `0x00011201`. `0x0001014B`
  has no owner. 2.4 GHz saves route to a registered handler and succeed.
- It is NOT the data (rates/channel/country are valid), NOT the web JavaScript, NOT the interface
  name. It is a missing/mis-routed handler in the compiled firmware.
- OSS address format (empirical): `0x0001 <moduleId:16>`. webd=1701, wlan_config=0301,
  wlan_mgr=1201, missing target=014B.

## Your objective
Make a 5 GHz save succeed, by **binary-patching** `webd`/`cspd` (and/or config) so the 5 GHz
config-apply reaches a handler that actually applies it. Two candidate strategies:
1. **Re-route**: patch webd so WLAN5–8 apply goes to the same handler that WLAN1–4 (2.4 GHz) use
   successfully — if that handler is radio-agnostic. Find webd's receiver-address computation
   (likely a table or `base + f(wlan_index)`), compare the value it produces for WLAN1 (works) vs
   WLAN5 (→0x014B), and redirect.
2. **Register**: find what should register module `0x0001014B` in cspd and why it doesn't, then
   fix that (the 2.4 GHz sibling registers — find the divergence).
Prefer whichever is smaller and more certain. **A no-change / same-length patch is strongly
preferred** (keep file sizes identical; patch bytes in place).

## Hard requirement: fallback before any device write
`telnetd` is a child of `cspd` (APPID 61, started by `cspd_misc`) — **a bad cspd patch loses my
telnet/shell**, recoverable only via the USB flash image. Before writing any patched binary to the
device:
1. Copy the original aside on-device (`cp /bin/cspd /bin/cspd.orig`, same for httpd).
2. Install a **cspd-independent boot watchdog** (early in `/etc/init.d/rcS`, which is on the
   remountable jffs2 rootfs): on boot, if a "patch-ok" marker isn't set within ~90 s (i.e. the
   device isn't healthy: cspd running AND telnet reachable AND a 5 GHz save works), restore the
   `.orig` binaries and reboot. Test the watchdog triggers correctly *before* relying on it.
3. Only then deploy the patch. Verify, and confirm the watchdog would have caught a failure.

## Assets already in this repo (offline-capable)
- `backup/bin/cspd` — stripped ARM ELF, 2.4 MB, md5 `633cf851b371924d1e25ee179051b7be`.
- `backup/bin/httpd` — the web daemon (webd), stripped ARM ELF, 1.1 MB, md5 `cef6d5485177fa658c1cde88f8d5c7a8`.
- `backup/firmware/mtd0_wholeflash.bin.gz` — whole flash, md5 (uncompressed) `7dc74ca9dbf79bde4f245cdf13078f5f` — the recovery image (also on the USB stick).
- `backup/device/db_user_cfg.xml` etc. — live/default configs.
- `zte670/` toolkit + `tools/tftpd.py`.

## Device access
- Device `192.168.1.1`; this Mac is `192.168.1.100` on the same link (Wi-Fi alias — if gone:
  `sudo ifconfig en0 alias 192.168.1.100 255.255.255.0`).
- Web: `admin`/`admin` (login token increments — read `Frm_Logintoken` value the page's JS sets).
- Telnet: `root`/`Pon521`. Drive it with `python3 -m zte670 telnet "<cmd>"`.
- Binary transfer: `python3 tools/tftpd.py --dir <dir> --port 6969 &`, then on device
  `tftp -p -l <f> -r <f> 192.168.1.100 6969` (device→Mac) or `-g` (Mac→device). `dd` is absent; use `cat`.
- Native debugger: `sidbg` — `sidbg 1 -p` (cspd module/PCB table), `sidbg 1 <mod> -l 8` (raise log
  level), `sidbg -pc show` (program manager). Logs: `/var/log/tmplog1`, `/userconfig/cfg/log`
  (read with `strings`). **Reset log levels to 5 when done** so flash doesn't fill.
- `/` is read-only jffs2 but `mount -o remount,rw /` works; `/etc` and `/bin` are overlays on
  `/opt/upt/apps` (mtd12), so edits persist.

## Reproduce the failure (to test any fix)
Trigger a 5 GHz save and read the result (expect FAIL/-258 until fixed):
```
# login (dynamic token), then POST net_wlan_essid_t.gch with IF_VIEWID=IGD.LD1.WLAN5,
# IF_ACTION=apply, _SESSION_TOKEN=<from page>, ESSID/MaxUserNum/etc. — see repo git history
# for the exact curl. A 2.4 GHz (IGD.LD1.WLAN1) save is the working control.
```

## Guardrails
- Confirm each risky/irreversible device write before doing it; keep me informed.
- Never leave the device without a working recovery path (watchdog + USB image).
- This is personal; capture findings in the `zte-f670l` okf bundle
  (`~/Documents/Personal/Knowledge/zte-f670l`, CLI only, `okf validate --strict`, push).
- If after a bounded effort a safe patch isn't found, say so plainly — don't ship a risky guess.

Start by loading REA and locating, in `backup/bin/httpd`, the code that produces receiver address
`0x0001014B` for a WLAN5 apply (search for the constant, the `0xAE09` event, and the
`OBJ_WLANCONFIG` path), and compare it to the WLAN1 path.
