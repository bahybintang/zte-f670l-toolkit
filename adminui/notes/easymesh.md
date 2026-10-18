# EasyMesh on the F670L clone

Status (2026-10-17): **working** as controller and as agent. Verified as an agent of a TP-Link
Archer AX12 controller over an Ethernet backhaul (LAN4): the ZTE finds the controller, completes
WSC M1/M2 for both radios and takes the controller's SSIDs and security.

## The stack

The firmware ships MediaTek's EasyMesh (Multi-AP) userland but never starts it:

| Binary | Role |
|---|---|
| `wapp` | bridge to the Wi-Fi driver (radio/BSS info, applying settings) |
| `p1905_managerd` | IEEE 1905.1: discovery, autoconfig search, WSC M1/M2. `-r0` controller, `-r1` agent |
| `mapd` | steering, channel planning |
| `1905ctrl`, `mapd_cli`, `wappctrl` | control CLIs (`1905ctrl <controller\|agent> dump_topology_info`) |

Drivers: `mt7603e_ap` (2.4 GHz, `wlan0..3`), `mtk7613e_ap` (5 GHz, `wlan4..7`). Both answer
wapp's WAPP queries (RT_PRIV_IOCTL 0x8BE1, OID_WAPP_EVENT 0x0647). Source for `wapp`/`mapd` is
public in the ASUS GPL tree (`RMerl/asuswrt-merlin.ng`, `release/src/router/wapp`, `.../mapd`).

## What adminui does (server/src/mesh.rs)

- **Config**: the daemons read fixed paths in `/wlan` (persistent jffs2, next to the radio
  calibration). Those copies are SDK templates. adminui generates `map_cfg.txt`, `wifi_info.txt`,
  `mapd_cfg.txt`, `wts_bss_info_config` and `wapp_ap.conf` into `/var/mesh` (tmpfs) and
  **bind-mounts** them over `/wlan/…`. Flash is never written; a reboot reverts.
- **Start**: `wapp` (with the two fixes below) → `p1905_managerd -r0|-r1` → `mapd`; stop is the
  reverse plus unmount. Restarted at boot when enabled (`mesh.json` in the data dir, 0600).
- **Status**: daemon liveness, agent onboarding (controller AL MAC, configured) and the parsed
  1905 topology (read back from `p1905.log`, which is block-buffered: dumps are requested until
  one complete dump has been flushed).

## The two fixes wapp needs

1. **Event layout (`mesh/wappfix.c`, LD_PRELOAD).** The driver sends WAPP events packed —
   `len(1) event_id(1) ifindex(4) data` — but this `wapp` build reads `ifindex` at +4 and data at
   +8 (its dispatcher: `ldr r1,[ev,#4]`, `add r2,ev,#8`). Every response was misparsed
   (`dev_type = 49, mac = 41:3d:…, ifname = an4` for wlan4), so no radio got a type/channel,
   `wifi_info` reported `inf_count=0`, and the agent never sent M1. The shim hooks `recvfrom` on
   netlink sockets and inserts two bytes after `event_id` in IWEVCUSTOM events with flags 0x0647,
   fixing every enclosing length. Requests (`wapp_req`) already match and are not touched.
   Built by `server/build.rs` with `zig cc -target arm-linux-gnueabi.2.26` and embedded.
2. **`wifi_config_save` (`mesh/wifi_config_save`).** wapp applies the controller's settings by
   running `wifi_config_save <if> <Key> <Value>`, which this firmware lacks. The shim applies them
   with `iwpriv <if> set Key=Value` and never logs values. It is put first on wapp's `PATH`.

## Wi-Fi backhaul (server/src/backhaul.rs)

An agent too far from the controller for a cable joins the controller's 5 GHz network with its
station interface (`apcli1`; `apcli0` is 2.4 GHz) and bridges it into br0. Four firmware traits
decide how (all verified 2026-10-17/18):

- **ZTE's bridge-port table** (`/proc/net/brport`) is filled for eth0-3 and wlan0-7 only. An
  unregistered station forwards nothing (permissions 0), and `brctl delif` of one **hangs the
  kernel until the hardware watchdog reboots the router**. Register first:
  `brctl setbrport br0 apcli1 13 1` and `brctl setpermits br0 13 ffffffff` (apcli0 = 12).
- **The Wi-Fi hardware-forwarding switch** (`/proc/wifi_hdswitch/on`, 1 by default; it sets
  `g_hdfwswitch` in `mt7603e_ap.ko`). While on, the drivers pass received frames to
  `wifihdswitchhandler[idx](skb, idmdev[idx])`, which files every frame under the radio's first
  AP: the station's traffic enters br0 as **wlan0**, bridged or not. The bridge then learns the
  controller's side behind wlan0 and sends replies out of the 2.4 GHz AP, and a station associated
  next to a cable makes a loop. adminui writes 0 while the backhaul runs (frames then arrive on
  apcli1; FDB and two-way traffic verified) and 1 once the stations are off.
- **Kernel loop detection** (`/proc/net/loopback/test`, probes with ethertype 0xFFFA; the
  `LoopbackGlobalConf` setting does not reach it) shuts a LAN port for 300 s when its probe comes
  back. The dmesg line `Dev_name=[wlan0]  bIfName=[eth0]` is that detector catching a probe.
- **No STP on br0.** So the station is never up alongside a cable to the controller: on a new LAN
  link, a loop-shut port or the controller's MAC behind a LAN port, the station is switched off;
  it reconnects only after the controller has been absent from the LAN ports for 120 s (judged
  while the station is off, because a loop-shut port hides the cable).

Testing without unplugging: `siswitch -spen 3 0` disables LAN4 in the switch (`-spen 3 1`
restores it). `ifconfig eth0 down` does not: the switch keeps delivering frames on eth0.

The firmware's DHCP server also ignores `DHCPSHostCfg.ServerEnable=0`; `server/src/languard.rs`
blocks DHCP (and, for a mesh agent, DHCPv6 and router advertisements) on br0 with firewall rules.

## Things that looked like the cause and were not

- "The 2.4 GHz driver has no MAP": `iwpriv wlan0 set MapEnable=1` fails, but the driver answers
  the WAPP query fine; MAP mode in the radio profile is not needed.
- `nvram_get` hanging under `popen`: an artefact of a tracing preload that wrote to stderr; the
  firmware's `/bin/nvram_get` answers wapp's lookups (`bss_config_priority`, `MapMode`, …).

## Safety notes

- `wapp`'s own output contains the received Wi-Fi passphrase; adminui sends it to `/dev/null`.
  `p1905.log` / `mapd.log` (0600, capped at 256 KiB, opened in append mode so the cap can
  shrink them) carry no key material.
- Debugging: `LD_PRELOAD` a `setvbuf(stdout, _IOLBF)` constructor to get live logs, and
  `wappctrl <if> wapp wdev_list` to see wapp's devices (dev_type 0 = not populated).
