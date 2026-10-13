# adminui

A replacement admin web UI for the ZTE F670L (and its YYKF clone), served **by the router
itself** as one static binary. It runs on `http://192.168.1.1:8088` alongside the stock UI
on port 80. The stock UI stays as a fallback.

- **Backend:** Rust (axum, current-thread tokio), `armv7-unknown-linux-musleabi`, about 3 MB
  with the web app embedded.
- **Frontend:** React, Tailwind, shadcn/ui (Base UI) and TanStack Query/Router. Built by Vite,
  pre-compressed (gzip and brotli) and embedded with `rust-embed`.
- **Pages:** 125 pages declared as data in [`schema/pages/*.json`](schema/SCHEMA.md). The
  same files drive the server's write whitelist and validation and the web forms.

## How it changes settings

| Step | What happens |
|---|---|
| Validate | Every value is checked against the page schema, on the server as well as in the browser. |
| Snapshot | `/userconfig/cfg/db_user_cfg.xml` is copied to `backups/` (the last 5 are kept). |
| Write | Each value goes through `sidbg 1 DB set`, then is read back. `sidbg` exits 0 even on failure, so a mismatch rolls back everything already written. |
| Save | `sidbg 1 DB save` persists the change to flash. |
| Apply | Most pages apply **live**: adminui sends the firmware's own change notification (OSS event `0x2401`) to the manager that owns the setting, the same message the stock UI sends, and treats the change as live only when the manager confirms. Wi-Fi SSIDs and passwords also go through `iwpriv`. Pages the firmware has no live path for are saved and take effect at the next restart; a banner offers **Restart now**. |
| Record | Each change goes into the change history, with undo and backup restore. |

adminui builds the `0x2401` message itself from the firmware's settings definition
(`/etc/db_tbl_def.xml`) and the values it reads back, with no vendor libraries and no shared
memory, and refuses to send if the message size differs from what the firmware's own database
reports. Most writable pages apply live this way — the DHCP server, WAN PPPoE, firewall level and
attack protection, static routes and Wi-Fi among them (the firewall rule lists apply live too, by a
different event — see below). In all, 46 of the 77 writable pages apply live. See
[`notes/oss-protocol.md`](notes/oss-protocol.md) §9 for the full list and for why the rest — WAN
IPoE, the LAN bridge and most VoIP — still need a restart. Each page states whether it applies
live, partly live, or on restart.

The **firewall rule lists** (IP/MAC filters, service control, DMZ, port forwarding) do not take a
0x2401 notify and a raw DB write never renders a rule — the firewall applies them through its own
**register event**. adminui sends that event (delete + add) after the write, generating the rule's
identity and resolving its interface the way the stock UI does, so rules added, edited and deleted
here take effect immediately. IP filtering, service control and MAC filtering are verified live on
device; DMZ and port forwarding use the same path but need an active WAN connection to appear in the
NAT table. See [`notes/oss-protocol.md`](notes/oss-protocol.md) §9 for the register protocol.

**WAN, LAN address, routing and firewall pages are commit-confirm.** After the change takes
effect (live, or at the next boot), you have **3 minutes** to keep it. If you don't, adminui
undoes it and, for boot-applied changes, restarts the router again.

Writes that would remove the recovery path (turning LAN telnet off) are refused unless you
explicitly acknowledge them.

## Build and deploy

Requirements on the Mac:
- `brew install rustup zig`, then `rustup default stable`
- `rustup target add armv7-unknown-linux-musleabi`
- `cargo install cargo-zigbuild`
- Node 20 or newer with npm
- The Mac at `192.168.1.100` on the router's LAN

```bash
cd adminui/web && npm install && cd ../..
adminui/scripts/deploy.sh                 # build web + server, tftp, install, restart, verify
adminui/scripts/deploy.sh --skip-web      # server-only change
```

`deploy.sh` keeps the previous binary as `adminui.prev`. On the first install it saves the
config as `backups/initial-<time>.xml`.

### Developing the UI

```bash
cd adminui/web && npm run dev     # http://localhost:5173, /api proxied to the router
```

The server tests run on the Mac with `cd adminui/server && cargo test`. The schema test fails
if a page file doesn't parse or names a table the DB doesn't have.

## On the router

| Path | Purpose |
|---|---|
| `/opt/upt/apps/adminui/adminui` | the binary (`serve`, `passwd`, `version`) |
| `/opt/upt/apps/adminui/adminui.sh` | supervisor: `start`, `stop`, `restart`, `status` |
| `/opt/upt/apps/adminui/adminui.conf` | login (Argon2id hash) |
| `/opt/upt/apps/adminui/backups/` | config snapshots |
| `/etc/rcS.d/S99zadminui` | boot hook, kept by the persistent `/etc` overlay |

- **Sign in:** `admin` / `admin` at first. The UI asks you to change it.
- **Forgot the password:** over telnet, run `/opt/upt/apps/adminui/adminui passwd`, then
  `/opt/upt/apps/adminui/adminui.sh restart`.
- **Turn it off:** `touch /userconfig/adminui.disable`. To remove it completely, run
  `rm /etc/rcS.d/S99zadminui` and `rm -r /opt/upt/apps/adminui`.

## Security

- The server listens only on the LAN bridge address (`br0`).
- Sessions are HttpOnly, `SameSite=Strict` cookies. Every state change needs a CSRF header, and
  cross-origin requests are refused. Repeated failed sign-ins lock that address out for a minute.
- Commands run with an argv and never through a shell. Host names and DB identifiers are
  validated first.
- Passwords stored in the router config are write-only: the API never returns them.
