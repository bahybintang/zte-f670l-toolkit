# ZTE F670L (clone) — OSS config-apply protocol spec

How the stock web server (`httpd`/"webd") makes a cspd config module apply a change **live**,
reconstructed so a root Rust program on the router can do the same without the web UI.

Scope: static RE only (Ghidra 12.1.4 headless + objdump) of local backups
`backup/bin/{httpd,cspd}` and `backup/lib/{liboss,libplib,libpmapi,libdbview,libdbcorecfg,libdbusc}.so`.
The router was **not** contacted. Confidence is marked per claim:
**[V]** = read directly in disassembly/decompilation, **[I]** = inferred/consistent with the
live trace in `~/Documents/Personal/Knowledge/zte-f670l/`.

Worked example used throughout: a stock SNTP apply that changed `PollTimeInterval`, which the
live trace saw as `webd(0x00030101) → sntp_mgr(0x0001010D) event 0x2401 MsgLen 1248`, plus
`→ DB(0x00010201) event 0x2101 len 8`.

--------------------------------------------------------------------------------
## 0. The call chain (top to bottom)

```
Lua page:  cmapi.setinst("OBJ_SNTP_ID", "IGD", {PollTimeInterval="86401", ...})   [V: gch/lua]
  httpd CMAPI dispatch: look up OBJ_SNTP_ID in the object table → pSetFunc           [V: strings]
    pm_sntp_set_config(identity="IGD", pstBuf)        libpmapi 0x001540b0            [V]
      zxicDbGetView(0xFA, "IGD", &cfg)      read current SNTP struct (0x4B8 bytes)   [V]
      ...patch changed fields in cfg...                                              [V]
      zxicDbSetView(0xFA, "IGD", &cfg)      persist to DB  → DB msg event 0x2101     [V call / I effect]
      pm_send_msg_2mm(0x0001010D, 0x2401, 0x80001, "IGD", &cfg, 0x4B8, 0, 0)         [V]
        pm_send_msg_2mm_with_timer_out(...)  libpmapi 0x00184cb4                     [V]
          SSEND(0x2401, appmsg, 0x4B8+0x28, timeout, replybuf, &seq, 0x0001010D)     [V]
            liboss SSEND 0x00015150 → AF_UNIX datagram, sync wait for reply          [V]
```

**The live apply is two messages, both emitted by the pm_ layer (which runs inside webd):**
1. `zxicDbSetView` persists the struct to the config DB (the `0x2101 → DB 0x10201 len 8`). **[V/I]**
2. `pm_send_msg_2mm(... 0x2401 ...)` notifies the owning **manager module** (sntp_mgr) to apply
   it live (the `0x2401 → 0x0001010D len 1248`). **[V]**

A raw `sidbg DB set`+`DB save` does step 1 only, so the module never re-reads → not live. That
matches the observed "DB changed but timezone file unchanged". **[I, explains prior finding]**

--------------------------------------------------------------------------------
## 1. OBJNAME → numeric id, DB view, module PID, event

`set_inst(OBJNAME, identity, paralist)` does **not** carry a numeric object id on the wire. httpd
resolves `OBJNAME` through a per-object descriptor table (function pointers `pSetFunc/pGetFunc/
pDelFunc/pQueryList/pQueryListByCond`, events `EV_CMAPI_SET/GET_VALUE/GET_NAME/ADD/DEL`) **[V:
httpd strings]** to a C setter `pm_<obj>_set_config`. That setter hard-codes the three numbers:

| thing | SNTP value | where | conf |
|---|---|---|---|
| DB **view id** | `0xFA` (250) | arg1 of `zxicDbGetView/SetView` | [V] |
| manager **module OSS addr** (receiver PID) | `0x0001010D` (sntp_mgr) | `DAT_00154fc8`, read as int | [V] |
| **event** (live-set) | `0x2401` | literal arg to `pm_send_msg_2mm` | [V] |
| **sub-id** (param "mm id") | `0x80001` | `DAT_001550c0` | [V] |
| identity / ViewName | `"IGD"` | rodata string (single-instance) | [V] |

Event codes seen in `pm_sntp_set_config`: **`0x2401` = config SET/apply** (struct→mgr);
**`0x2402` = a GET sub-query** (used to fetch the timezone/day-time table before building the
struct, payload 0x13C). **[V]** Add/Delete-instance use the `EV_CMAPI_ADD/DEL` events via the
same `pm_send_msg_2mm` mechanism; exact numbers not extracted. Other modules follow the same
shape, e.g. prior art: WLAN → `wlan_mgr 0x00011201`, event `0x2401` (`pm_wlan_set_config`). **[V prior]**

There is **no single generic "OBJNAME→(id,PID,event)" table to dump** — the mapping is distributed
as constants inside each `pm_*_set_config`. To enumerate it generically, read `DAT`/literal
operands of each `pm_*_set_config` (receiver addr = the int constant passed as arg1 to
`pm_send_msg_2mm`; view id = arg1 of `zxicDb*View`).

--------------------------------------------------------------------------------
## 2. OSS wire format (AF_UNIX datagram)

### 2.1 Sockets  [V: liboss `SocketCreate` 0x191e4, `SocketSynSendMsg` 0xa9c4]
- `socket(AF_UNIX=1, SOCK_DGRAM=2, 0)`; `sockaddr_un.sun_family = 1`; `bind/sendto` addrlen =
  **`0x6e` = 110** (`sizeof(sockaddr_un)`). Path template **`"/var/tmp/0X%08X"`**
  (string at liboss `.rodata`: `/var/tmp/` + `%s0X%08X`). Bound files are `chown`ed to
  **uid 108 / gid 110**; root (or that uid) may write. **[V]**
- **Address layout** (32-bit, the `%08X`): `0x00_SS_AA_MM` — `SS` = subsystem/node byte,
  `AA` = APPID, `MM` = MODID. In memory the word is little-endian (MM at the low byte). **[V/I]**
  Examples: `sntp_mgr=0x0001010D` (SS01 AA01 MM0D), `wlan_mgr=0x00011201`, `DB=0x00010201`,
  `webd=0x00030101` (SS=**03**).
- **Request (inbound) socket** a client sends TO: the sender reads **one byte at message offset
  6** = the receiver's `SS` byte and formats `/var/tmp/0X000000<SS>`. For every cspd-hosted
  module `SS=0x01`, so **all requests go to `/var/tmp/0X00000001`** (cspd's main OSS comm socket,
  bound by `InitComm` with key `node+SS` = 1). cspd then demuxes internally by the receiver
  field. **[V: `ldrb r3,[r4,#6]` at 0xa9f8; `InitComm` 0x19458 `SocketCreate(node+SS,0)`]**
- **Reply socket** a module sends back TO: `(sender & 0xFFFFFF) >> 8` = `SS<<8 | AA`, i.e.
  `/var/tmp/0X0000<SS><AA>`. For webd `0x00030101` → **`/var/tmp/0X00000301`**. Each endpoint
  binds this as its own receive socket. **[V: `SocketSynSendAckMsg` 0x1b20c sprintf arg
  `(*param_1 & 0xffffff) >> 8`; `OSSStart` 0x1cc8c `SocketCreate((addr&0xffffff)>>8 + node*0x100)`]**

### 2.2 The 32-byte OSS header  [V: `SSEND` 0x15150]
`SSEND(event, payload, paylen, timeout, replybuf, &seq, receiver)` allocates `paylen+0x20`,
zeroes 0x20, then fills (all little-endian):

| off | sz | field | value set by SSEND |
|----|----|-------|--------------------|
| 0x00 | 4 | **sender** OSS addr | self addr (`PCB+0x18c`); for socket send high16 OR'd with node |
| 0x04 | 4 | **receiver** OSS addr | `receiver` arg (e.g. `0x0001010D`) |
| 0x08 | 4 | **event \| 0x20000** | e.g. `0x2401 | 0x20000 = 0x00022401` (bit 0x20000 = SYN_REQ) |
| 0x0C | 2 | **payload length** | `paylen` (low 16) |
| 0x0E | 2 | **max reply length** | the module refuses to reply if its answer is longer (`OSSSendAckData`) — **corrected 2026-10-13**, it is not a sequence number |
| 0x10 | 2 | expected reply length | `SocketSynSendMsg` `recvfrom` size − 8; set equal to 0x0E |
| 0x12 | 2 | sender AppInst | global node/inst (`OSSSenderAppInst` reads this) |
| 0x16 | 2 | msg counter | per-PCB send counter (echoed in reply for matching) |
| 0x18 | 2 | 0 | |
| 0x1C | 4 | **replybuf ptr** | caller buffer; reply payload is memcpy'd here on return |
| 0x20 | .. | **payload** | `memcpy(hdr+0x20, payload, paylen)` |

On the wire the datagram = **32-byte header + payload**. For the SNTP example the OSS header's
length field = `0x4B8+0x28 = 0x4E0 = 1248` → matches the observed `MsgLen 1248`; the datagram is
`32 + 1248 = 1280` bytes. **[V: exact arithmetic `param_6 + 0x28` in `pm_send_msg_2mm_with_timer_out`]**

### 2.3 Sync request/reply  [V: `SocketSynSendMsg` 0xa9c4]
Cross-process sends are **synchronous**: `sendto(reqsock, hdr, 32+paylen, 0, {AF_UNIX,
/var/tmp/0X00000001}, 0x6e)`, then `select()`+`recvfrom()` on the sender's **own** bound socket
(`PCB+0x2c`). The reply **datagram** is an 8-byte reply header + data:

| off | sz | field | validated against |
|----|----|-------|-------------------|
| 0 | 2 | event (echo) | must equal request `event & 0xFFFF` |
| 2 | 2 | (unused) | |
| 4 | 2 | reply payload length | ≤ caller max |
| 6 | 2 | seq | must equal request header off 0x16 (the msg counter) |
| 8 | .. | reply payload | copied to `replybuf` (header off 0x1C) |

### 2.4 Application reply payload (iRet)  [V: `pm_send_msg_2mm_with_timer_out` 0x184cb4]
The pm_ layer's reply buffer is interpreted as:

| off | sz | field |
|----|----|-------|
| 0x00 | 4 | **`iRet`** (s32): `0` = success; negative = error (e.g. `-258` seen for the broken WLAN5) |
| 0x04 | 4 | reply data length |
| 0x08 | .. | reply data |

So success is the **first 4 bytes of the reply payload == 0**. `SSEND` itself returns the OSS
reply length; the module result is in that first word.

--------------------------------------------------------------------------------
## 3. The 0x2401 payload: per-object C struct, NOT a generic paralist

**[V]** The body passed to the manager module is a **fixed per-object C struct**, built by
`pm_send_msg_2mm_with_timer_out` as:

```c
struct mm_msg {               // total = 0x28 + datalen
    u32  sub_id;              // off 0x00  = pm "mm id" (SNTP: 0x80001)
    char view_name[32];       // off 0x04  = identity/ViewName, e.g. "IGD" (NUL-padded)
    u32  datalen;             // off 0x24  = struct size (SNTP: 0x4B8)
    u8   data[datalen];       // off 0x28  = the object config struct, copied verbatim
};                            // SNTP: 0x28 + 0x4B8 = 0x4E0 = 1248
```

The `data[]` blob is the DB-view struct: `pm_sntp_set_config` gets it with
`zxicDbGetView(0xFA,"IGD",&buf)` (`buf[0]=0x4B8` capacity header, then fields), patches changed
members, writes it back with `zxicDbSetView`, and ships the **same bytes** to the mgr. **[V]**

The ASCII **paralist** `name=value|name=value|...` format (`=`=0x3D, `|`=0x7C, ≤2047 B,
NUL-terminated, empty value = bare name) is real and fully generic — see
`plib_paralist_to_buffer` (0x16df4) / `plib_buffer_to_paralist` (0x16fc8) **[V]** — **but it is
the transport for the cmapi/TR-069/DB-view layer, not for the `0x2401` mgr-notify.** The mgr
expects the struct.

**Consequence for a generic encoder:** a *purely wire-level* generic encoder for `0x2401` is
**not feasible** without each object's struct layout, because `data[]` is a per-object struct and
field offsets differ per object. What *is* feasible generically is **read-modify-write**: the
struct is obtained verbatim from `zxicDbGetView`, so a client only needs the **offset of the one
field it changes**, not the whole layout. See §5.

--------------------------------------------------------------------------------
## 4. Simpler alternatives that make a module go live

- **Best: call the vendor libs directly.** A root Rust program can `dlopen("libpmapi.so")` and
  call `pm_sntp_set_config` (or `pm_wlan_set_config`, …) exactly as webd does — this performs the
  DB write **and** the `0x2401` notify, identical to the web UI, with zero wire-format risk.
  The lower primitives are also exported: `zxicDbGetView/SetView/QryView/DelView` (DB view
  read/write) and `SSEND`/`pm_send_msg_2mm` (the OSS send). **[V: all are exported symbols]**
- **DB read-modify-write + notify** (no per-object struct knowledge beyond one field offset):
  `zxicDbGetView(view,id,&buf)` → patch → `zxicDbSetView(view,id,&buf)` →
  `pm_send_msg_2mm(module,0x2401,subid,id,&buf,datalen,0,0)`. **[V, this is literally what the pm_
  setter does]**
- **No evidence of a single generic "DB changed / reload everything" broadcast.** The live-apply
  is always the targeted `0x2401` to the object's own manager module. `zxicDbSetView` notifies the
  **DB** module (`0x10201`, event `0x2101`) for persistence/view-subscribers, but on this build
  that alone does **not** drive the driver live (confirmed by the SNTP/WLAN behaviour). So the
  `0x2401` mgr-notify is required. **[V/I]**
- `zxicDbSetView`'s own body is **not in the backup libs** (imported/UND in all pulled `.so`s),
  so the exact `0x2101`→DB wire format is **unverified here**; treat "SetView ⇒ 0x2101 to DB
  len 8" as **[I]** from the live trace.

--------------------------------------------------------------------------------
## 5. Sender identity, impersonation, auth, risk

- **Sender field is caller-chosen; there is no cryptographic/registration auth on inbound
  datagrams.** cspd dispatches by the *receiver* field and replies to the socket named from the
  *sender* field. The only gate is filesystem perms on `/var/tmp/0X00000001` (chown uid108/gid110)
  — **root bypasses**. **[V: no auth in SSEND/ack path; chown in SocketCreate]**
- **Pick a sender whose `SS` byte ≠ 0x01.** A module's reply goes over the socket only when the
  sender's `SS` differs from cspd's node SS (0x01); otherwise it attempts an *in-process* reply
  via the shared PCB and will not find a foreign process. webd itself uses `SS=0x03`
  (`0x00030101`) for exactly this reason. **[V: `OSSSendAckData` 0x146a8 compares
  `(sender&0xffffff)>>16` to the node global]**
  - Safe recipe: sender = `0x0003xxyy` with an `AA/MM` not already bound. Bind your reply socket
    at `/var/tmp/0X0000<SS><AA>` (e.g. `0X00000302`) **before** sending; `select`/`recvfrom` it.
  - You *may* impersonate webd `0x00030101` only if webd is not currently bound to
    `/var/tmp/0X00000301` (else `bind` → EADDRINUSE). A distinct unused address is safer.
- **Registration is not required** to send; it *is* required to receive via the normal in-proc
  path, which is why you bind your own AF_UNIX socket and rely on the sync reply instead.
- **Risks:** a malformed struct or wrong `datalen` can crash the manager module → watchdog
  reboot; recovery here is watchdog-only (no physical reflash), so test on the spare device.
  An unexpected event/object just returns a negative `iRet` (observed inert, e.g. WLAN5 −258) —
  harmless. Keep `datalen` exact and obtain `data[]` from `zxicDbGetView` rather than hand-building.

--------------------------------------------------------------------------------
## 6. Byte-level example — set SNTP PollTimeInterval=86401, identity "IGD"

Addresses: receiver `sntp_mgr = 0x0001010D`, sender = `0x00030102` (chosen, SS=0x03, unused AA/MM),
event `0x2401`, sub-id `0x80001`, view `0xFA`, datalen `0x000004B8`.

**Step A — get the current struct** (so `data[]` is valid and only one field changes):
`zxicDbGetView(0xFA, "IGD", &buf)` → 0x4B8-byte struct. Patch the `PollTimeInterval` member
(a `u32`; its offset is in the SNTP struct — read it once from `pm_sntp_set_config`/`get_config`,
it is the field written next to the server-name strings) to `86401 = 0x00015181`.
`zxicDbSetView(0xFA,"IGD",&buf)` to persist.

**Step B — build the application message** (`0x28 + 0x4B8 = 0x4E0` bytes):
```
off 0x000:  01 00 08 00                      sub_id   = 0x00080001  (LE)
off 0x004:  49 47 44 00 00 ... 00            view[32] = "IGD\0" NUL-padded to 32
off 0x024:  B8 04 00 00                      datalen  = 0x000004B8  (LE)
off 0x028:  <0x4B8 bytes: the struct from Step A, with PollTimeInterval=0x00015181>
```

**Step C — build the 32-byte OSS header** (payload length = `0x4E0 = 1248`):
```
off 0x00:  02 01 03 00      sender   = 0x00030102  (LE)
off 0x04:  0D 01 01 00      receiver = 0x0001010D  (LE)  (byte at off 0x06 = 0x01 → request sock)
off 0x08:  01 24 02 00      event|0x20000 = 0x00022401  (LE)
off 0x0C:  E0 04            payload length = 0x04E0
off 0x0E:  01 00            seq = 1
off 0x10:  01 00            (copy of seq)
off 0x12:  00 00            AppInst (0)
off 0x14:  00 00
off 0x16:  01 00            msg counter = 1  (must match reply off 6)
off 0x18:  00 00 00 00
off 0x1C:  00 00 00 00      replybuf ptr (unused on the wire; in-process only)
```
Concatenate header(32) + application message(1248) = **1280-byte datagram**.

**Step D — send & read reply:**
```
reqfd  = socket(AF_UNIX, SOCK_DGRAM, 0)
bind(reqfd, "/var/tmp/0X00000302", 110)            # our reply socket = SS<<8|AA = 0x0302
sendto(reqfd, datagram, 1280, 0, "/var/tmp/0X00000001", 110)   # cspd's inbound router socket
select(reqfd, timeout); recvfrom(reqfd, rbuf, ...)
# reply: rbuf[0:2]=event echo (0x2401), rbuf[6:8]=seq (1); payload at rbuf[8:]
# application iRet = int32(payload[0:4])  → 0 means applied live
unlink("/var/tmp/0X00000302")
```

**How to read success:** OSS-level, `recvfrom` returns ≥ 9 and `rbuf[0:2]==0x2401` and
`rbuf[6:8]==1`. Application-level, the first 4 bytes of the reply payload (`iRet`) must be `0`.
A negative value (e.g. −258) means the mgr rejected the apply.

--------------------------------------------------------------------------------
## 7. Feasibility verdict

A **fully generic raw-wire `0x2401` encoder is not feasible** on its own, because the `0x2401`
body is a **per-object C struct**, not a generic TLV/paralist (the ASCII `k=v|` paralist exists
but belongs to the cmapi/DB layer). A **generic live-apply is feasible** two ways: (a) `dlopen`
the vendor `libpmapi.so` and call `pm_<obj>_set_config` / `zxicDb*View` directly — lowest risk,
behaviourally identical to the web UI; or (b) replicate the exact pattern — `zxicDbGetView`
(gives the opaque struct) → patch one field by offset → `zxicDbSetView` → `pm_send_msg_2mm(module,
0x2401,…)` — which needs only the changed field's offset, not the whole struct. The OSS transport
(AF_UNIX `SOCK_DGRAM`, 32-byte header, `/var/tmp/0X00000001` inbound, per-app reply socket,
synchronous reply with `iRet` word) **is** fully generic and documented above.

--------------------------------------------------------------------------------
## Generic re-apply via libpmapi (follow-up)

Goal: a struct-layout-free live apply for object X — `buf=0; pm_X_get_config(id,buf);
pm_X_set_config(id,buf)` — the get fills `buf` from the DB view, the set writes it back
(no-op) and fires the `0x2401` notify, exactly like webd. Files produced:
`adminui/notes/cmapi-table.json`, `adminui/helper/pmcall.c`.

### Q1 — the dispatch table  [method noted]
httpd does **not** hold a flat `OBJ_*`→handler pointer array: there are no data pointers to the
`OBJ_*` rodata strings, and the lookup uses split pc-relative loads Ghidra does not turn into
xrefs, so an exhaustive in-binary dump was not cheap to extract. **What is authoritative and
machine-checked:** the set of `pm_<x>_get_config`/`_set_config`/`_add_config`/`_del_config`
functions that are **both** imported by `httpd` **and** defined in `libpmapi.so` — i.e. the
handlers webd can actually call. That is `cmapi-table.json`: **31 stems, 29 with a get+set pair**.
Only these support the get→set re-apply. **Objects without a dedicated pm pair (WLAN, firewall,
WAN, …) use the generic cmapi/paralist path, not this mechanism** — consistent with the WLAN5
case. `OBJ_*`→stem is left `null` except the one verified by decompilation: **`OBJ_SNTP_ID` →
`pm_sntp` (view `0xFA`, module `0x0001010D`, structLen `0x4B8`)** **[V]**. To fill `obj/viewid/
module/structLen` for another stem, decompile its setter: view id = arg1 of `zxicDb*View`,
module = int const passed arg1 to `pm_send_msg_2mm`, structLen = the `memset`/`local=0xNNN` size
before `zxicDbGetView`.

### Q2 — pm_* signatures  [V single-instance; I multi-instance]
Uniform shape (verified on `pm_sntp_get_config` 0x1550d0 and `pm_sntp_set_config` 0x1540b0):
```c
int pm_<x>_get_config(const char *identity, void *struct_buf);   // buf filled from DB view
int pm_<x>_set_config(const char *identity, void *struct_buf);   // DB write + 0x2401 notify
```
- `struct_buf` is the **parsed pm struct** (NOT the raw DB view blob); fields are written by
  `pm_bool_prm_op`/`pm_str_prm_op` into `buf+offset`. Must be caller-zeroed; sizes seen < 0x600,
  so the helper's 64 KiB buffer is safe. NULL `buf` → returns `-10`/`-0xA`. **[V]**
- Return `0` = success; negative = error (e.g. `-258`). **[V]**
- **identity:** for single-instance objects it is `""`/`"IGD"` and the setter hard-codes the
  ViewName; for **multi-instance** objects the identity (`"IGD.LD1.WLAN5"`, `"IGD.FWDMZ.FWDMZ1"`)
  is the instance path that becomes the 32-byte **ViewName** field of the `0x2401` message
  (Section 3) and the `zxicDb*View` key. Treat multi-instance identity flow as **[I]** (not
  re-decompiled per object). Some objects take extra mask/flags args (not uniform) — **verify
  the specific setter before trusting the 2-arg shape**; `add`/`del` handlers differ.

### Q3 — process init a standalone caller needs  [V, this is the real gate]
`pm_*_set_config → pm_send_msg_2mm → SSEND`, and `SSEND` requires this process to own an OSS
**PCB** (`GetSelfThreadPCBPtr()` must be non-NULL) and a bound reply socket. **webd imports and
calls `OSSInitConfig` + `OSSStart`** (full OSS *node* init with a task table) — `strings`/imports
confirm; it does **not** import `InitAppComm`. So:
- A standalone helper that calls `pm_*_set_config` **must bring up OSS first.** The lightweight
  `InitAppComm(appid_base, node)` (liboss 0x24e34) binds one AF_UNIX socket and sets the self
  address = `appid_base + 0x101`, but it does **not** create the PCB that `SSEND` dereferences,
  so on its own it may be insufficient for the *send* (it is enough for the reply socket and for
  a raw-datagram sender). If `set()` returns a send error, the process must instead call
  `OSSInitConfig(cfg)` then `OSSStart(task_table, x, node)` with a minimal 1-task table. **[V/I]**
- **Recommended sender identity:** choose a subsystem byte **not** present in `sidbg 1 -p`
  (live module table). `0x01` = cspd modules, `0x03` = webd, `0x3d`(61) = telnetd family — avoid
  these. The helper uses `InitAppComm(0x7E0000,0)` → self `0x7E0101` (subsystem `0x7E`, reply
  socket `/var/tmp/0X00007E01`). Requests still go to `/var/tmp/0X00000001`; subsystem `!= 0x01`
  guarantees the module replies over the socket (Section 5). **[V reasoning]**
- **The fully self-contained alternative** (no libpmapi init at all) is the raw `0x2401` datagram
  of Section 6: bind one socket, `sendto` `/var/tmp/0X00000001`, read the `iRet`. It needs the
  struct bytes, which `pm_<x>_get_config` (a DB read) can supply if it runs without OSS init,
  else read them from the decrypted DB as adminui already does.

### Q4 — the helper
`adminui/helper/pmcall.c`: `pmcall reapply <stem|fn> <identity>` (dlopen `libpmapi.so`
RTLD_GLOBAL, dlsym get+set, `get` then best-effort `InitAppComm` then `set`, prints `OK`/`ERR
<iRet>`) and `pmcall get <stem|fn> <identity>` (hex-dumps the struct). Defensive: all dlsym
NULL-checked, 64 KiB zeroed buffer, no shell/exec. **Build (verified to compile with the
installed `zig` 0.17.0):**
```
zig cc -target arm-linux-gnueabi.2.17 -O2 -o pmcall pmcall.c -ldl
```
Target rationale, from the backup binaries: `/lib/ld-linux.so.3` ⇒ **soft-float armel** (not
`armhf`, so `gnueabi` not `gnueabihf`); max required symbol version is **GLIBC_2.17** (most are
2.4/2.7). The local build produced `ELF 32-bit ARM EABI5, interpreter /lib/ld-linux.so.3` —
matches the device. **Not run** (per instructions); the binary was deleted, source kept.

### Q5 — risks of get→set re-apply
- **Round-trip is only safe if the struct is pure config.** If `pm_X_get_config` populates
  runtime/status fields (counters, link state, negotiated values, lease times) that
  `pm_X_set_config` then persists, a re-apply can overwrite good runtime state or re-trigger side
  effects. SNTP's struct is pure config → safe. **[V for SNTP; I in general]**
- **Dangerous to re-apply** (side effects on set): anything WAN/connection (PPP/DHCP
  renegotiation, link drop), **firewall / NAT / port-forward** (`pm_fw_*` — rule flush/reorder),
  routing, and login/telnet objects. Most of these are **not** in `cmapi-table.json`'s get/set
  set anyway. Within the 31: WAN-side multicast (`pm_igmp_wan`, `pm_e8_igmp_wan`,
  `pm_igmp_proxy`, `pm_mld_proxy`, `pm_vlanbind`, `pm_iptv`) can blip multicast/IPTV; `pm_dns`,
  `pm_ddns`, `pm_bandwidth`, `pm_upnp`, `pm_log`, `pm_sntp`, `pm_tracert`, `pm_arp_detect` are
  low-risk. **Multi-instance identity must be exact** or you apply to the wrong instance.
- **Crash risk:** `pm_send_msg_2mm` ships a struct to a manager module; a wrong struct/length can
  crash it → watchdog reboot (no physical reflash here). The get→set flow keeps the struct
  self-consistent, which is its main safety advantage over hand-building. Test on the spare unit,
  one object at a time, and check the printed `iRet`.

--------------------------------------------------------------------------------
## 8. The view struct is generic — built from `/etc/db_tbl_def.xml`  [V 2026-10-13]

The `0x2401` `data[]` blob (the "DB view") does **not** need per-object reverse engineering.
`dbcGetView` → `dbGetCommView` → `dbViewCommOp` (libdb.so) builds it generically by walking the
table's DM list (the `<DMDEF>` order in `/etc/db_tbl_def.xml`) and emitting one fixed-size **slot**
per DM. Verified against `pm_sntp_get_config`'s offsets (SNTP: 19 DMs → 1200 B + 8 = `0x4B8`, which
equals `sidbg 1 DB pv` Len 1208 and the `getSntpViewParam` length).

**View** = `u32 total_len` (= the `Len` from `pv` / `get<X>ViewParam`) · `u32 magic 0x01020304` · slots.

**Slot** (size = `dbGetPmTypeValLen(type)` = 28-byte header + value, see table):

| off | sz | field |
|----|----|-------|
| 0x00 | 2 | DB type code (table below) |
| 0x02 | 2 | changed flag: 0 from GetView; the pm setters set **1** on each field they changed |
| 0x04 | 20 | 0, except the view-name DM, which gets its DM name here (`strncpy` 15) |
| 0x18 | 4 | s32 op result (0) |
| 0x1C | .. | value: STRn = NUL-terminated text; INT32/DWORD/ENUM/BOOL = LE u32 (BOOL read as u8); IP/IPV6/MAC/BUF16 = raw row bytes |

**Type codes and slot sizes** (cspd type-name table at `0x240320` = `{code,name}` pairs;
libdb `dbGetPmTypeValLen` u16 table):

| code | XML type | slot | | code | XML type | slot |
|---|---|---|---|---|---|---|
| 0 | STR8 | 40 | | 10 | INT32 | 32 |
| 1 | STR16 | 48 | | 11 | DWORD | 32 |
| 2 | STR32 | 64 | | 12 | ENUM | 32 |
| 3 | STR64 | 96 | | 13 | BOOL | 32 |
| 4 | STR128 | 160 | | 14 | IP | 32 |
| 5 | STR256 | 288 | | 15 | IPV6 | 44 |
| 6 | STR512 | 544 | | 16 | MAC | 36 |
| 7 | STR1024 | 1056 | | 18 | BUF16 | 44 (pointer-backed) |
| 8 | STR2048 | 2080 | | | | |
| 9 | STR4096 | 4128 | | | | |

The pm setters' own type writes confirm codes 10, 11 and 13 (`pm_int32/dword/bool_prm_op`).
**Not yet verified:** the byte order of IP values (raw row memory, probably network order) and
BUF16; avoid views with those types until a byte-level check exists.

**Safety finding (2026-10-13):** running `pm_sntp_get_config` in a standalone process after
`DBShmCliInit` (attaches the DB shared memory read-write at `0x60000000`) crashed it, and the whole
userland (cspd hosts ~60 managers) died: ping answered, but every TCP port was refused until a
power cycle. The csp locks use `SEM_UNDO` (`cspLock` `semop` flags 0x1800/0x1000), so a stranded
lock is not the cause. Never attach the DB shm or call `zxicDb*`/`pm_*` from a foreign process.
Build the view from `db_tbl_def.xml` and `sidbg` values and send it over the socket (§2) instead.

**Verified end to end (2026-10-13, adminui `oss.rs`):** with 0x0E/0x10 = 2048, sender
`0x0003AA01` and a world-writable reply socket `/var/tmp/0X000003AA` (cspd runs as user csp),
an SNTP view built from `db_tbl_def.xml` + `sidbg` values is accepted by `sntp_mgr`, which replies
`iRet 0` and applies it live (`sidbg 1 sntp_mgr getGlobalVar` → `g_dwpoll_time` follows the DB
value). Setting 0x0E to 1 made the module apply the change but drop its reply.
Targets for all 34 libpmapi setters that send `0x2401`: `notes/pm-targets.json`.

--------------------------------------------------------------------------------
## 9. Live-apply coverage (2026-10-13, verified on device)

**41 of 82 writable pages apply live.** The enabled build-macro set was solved from every view's
length (ENABLED_MACROS in oss.rs), so views the all-or-none rule could not size — LAN, admin
users, log, WAN, DHCP host/pool/bind — now build correctly; `layout_for` filters DMs by that set
and still length-checks, so a wrong pick is impossible.

Live targets (each iRet 0 on device, or a safe no-row fallback here): SNTP, DNSSettings,
ARPDETECTConfig, IGMPProxy, MLDProxyCfg, MLDWan, FWALG, FWBase, FWLevel, RIPConf, RIPIf,
PortControl, PrefixCfg, QOSRule, QOSType, QOSBasic, QOSQueue, QOSPolicer, DDNSService, DDNSClient,
DNSHostsList, DHCPSHostCfg, DHCPSPool, DHCPSBind, DHCPCComm, LAND, DevAuthInfo, Log, TelnetCfg,
TelnetUser, WANC, WANCIP, WANCPPP, LoopbackEthConf, L3ForwardingRT, RaCfg, TUNNEL46CFG, VLANBIND,
FWURL, SambaCfg, WLANBANDSTEERING, WLANROAMING, WLANWMM, WLANWEP, WLANWPS, WlanTimeCfg. Plus Wi-Fi
SSID/PSK via the iwpriv hook.

**Firewall rule lists — live via the register event** — MacFilter (244), FWSC (189), FWDMZ (238),
FWIP (239), FWPM port forwarding (27). These do **not** take a 0x2401 view notify; a raw DB write
does not render a rule (verified: empty iptables after write and after reboot). They are applied by
the firewall's own **register event**, which adminui now sends — no reboot, no vendor libs, no stock
UI. Implemented as `oss::fw_struct` / `fw_register` / `fw_delete` / `send_raw` / `datagram_ev`, wired
through `hooks::firewall` and `apply.rs`, with the `oss-fwreg <table:row>` CLI.

Transport: the stock setters (`pm_fwset_ip` @libpmapi `0x5bc18` etc.) write the rule, then call
`plib_fw_schudle_register_api(event, struct428)` (libplib `0x22150`) =
**`SSEND(event, struct, 428, 5000, replybuf, &seq, receiver=0x10102)`**. Module `0x10102` is hosted
in cspd (subsystem byte 0x01), so its request socket is cspd's `/var/tmp/0X00000001`, exactly like
the 0x2401 targets; the reply carries iRet at payload+0 (0 = accepted). **0x9704 deletes, 0x9703
adds** (all five tables share the pair; event also mirrored into struct[156]).

The 428-byte struct is NOT the rule — `pm_fwset_ip` copies only a few fields out of the getFwipParam
view (1480 bytes, 20 §8 slots, `getFwipParam`@libdbview) and the firewall **re-reads the match
fields (IPs/ports/protocol/target) from the DB by ViewName**. So the struct carries only identity +
interfaces + enable:
- struct[0] ← slot0 **ViewName** (view off 0x24), strncpy ≤34
- struct[36] ← per-table **type constant** (word): FWIP `0x000b0008`, FWSC `0x000b000e`,
  FWDMZ `0x000b0003`, FWPM `0x000b000a`, MacFilter `0x000b0011` (each setter's literal pool)
- struct[76] ← the **IN/primary connection ViewName** (FWIP/FWSC `INCViewName`, FWDMZ/FWPM
  `WANCViewName`; MacFilter none), strncpy ≤34
- struct[112] ← the **OUT connection ViewName** (FWIP `OUTCViewName`; others none)
- struct[148] ← **Enable** (one byte)
- struct[156] ← the event (0x9703/0x9704) as a halfword
- struct[160]/[164] are past the last slot (zero) — the "259-byte descriptor" is empty; an earlier
  note that mistook this for a mandatory rule descriptor was wrong.

**adminui's add path** (apply.rs) mirrors the OBJ layer for a new rule: it generates a `ViewName`
(`au<hex>`), and resolves the interface `Name` from the chosen connection ViewName
(`oss::resolve_conn_name`: LAND `IGD.LD1`→`br0`, or WANC→its device) and stores it, since a rule
without the resolved Name matches on any interface. Delete fires the 0x9704 **before** the DB row is
removed (the firewall re-reads the row to find the iptables entry).

**Validated on device (spare F670L clone):**
- **FWIP** — full adminui API round-trip: add → `DROP … br0 … source IP range` in iptables
  `ipfilter`; delete → removed; disable/enable toggles the rule. `apply:"live"`.
- **FWSC** — register renders rules in iptables `srvcntrl`.
- **MacFilter** — register renders in **ebtables** (bridge MAC filter, `-s <mac> --dir-wan`), once
  `FWBase.MacFilterEnable=1` (the page's global toggle, itself a live 0x2401 target).
- **FWDMZ / FWPM** — register accepted (iRet 0) but **not render-verifiable on this device**: they
  are NAT rules and the WAN is down here (`WANC Status=0`, no ppp/nas iface). Same mechanism and
  type constants as the validated tables; adminui resolves `WANCName` on add. Expected to render
  once a WAN connection is up.
- **No 0x2401 sender / no sync reply** — MultiWancd (iRet -5), LoopbackGlobalConf (iRet -1),
  DMSCfg / CltLmt / UPnPCfg / WANCIP-via-IP-manager (no reply), DDNSHostname (gdbus),
  WANCD/WANCEXT/WANCServList/WANCPPPComm, DevInfo, MonitorCollectorCfg, MirrorConf, UsbBakRst,
  DHCP6SHostCfg and DWQOSQueue (view-id collisions). The 0x10111 telnet-user subid **crashed** the
  box, so the 0x3d0101 telnet node is used instead.
- **VoIP and TR-069** — excluded by request.

## 10. WAN manual control (PPPoE connect/disconnect, DHCP renew/release)

The stock WAN pages call `dev_action` with `CmdId=cmd_pppconnect | cmd_pppdisconnect |
cmd_dhcpconnect | cmd_dhcpdisconnect` and `Identity=<WAN ViewName>` (`ethwancppp_gch.gch`,
`ethwancip_gch.gch`). webd's dispatch table (httpd literal pool @0xb1738, `{name, handler}` pairs)
maps them to libpmapi exports, each a single `pm_send_msg_2mm`: **[V: disassembly]**

| CmdId | libpmapi | module | event | subid | data |
|---|---|---|---|---|---|
| cmd_pppconnect | `pm_wanc_ppp_manual_connect` @0x126944 | 0x10109 | 0x2801 | 0 | WANCPPP view (`zxicDbGetView(20, id)`), len 0x708 |
| cmd_pppdisconnect | `pm_wanc_ppp_manual_disconnect` @0x126aa4 | 0x10109 | 0x2802 | 0 | same |
| cmd_dhcpconnect | `pm_wanc_ip_manual_connect` @0x126c04 | 0x10c01 | 0x2801 | 0 | none (len 0) |
| cmd_dhcpdisconnect | `pm_wanc_ip_manual_disconnect` @0x126ce4 | 0x10c01 | 0x2802 | 0 | none |

The message is the §2 application message (subid, 32-byte identity, len, data) under a different
event. On this build the WANCPPP view (id 20) is exactly 1800 = 0x708 bytes, so adminui's rebuilt
view equals the stock buffer. Both modules live in cspd (socket `/var/tmp/0X00000001`).
DHCP renew/release go through the 0x10c01 manager; there is no udhcpc signal path.

adminui: `oss::wan_manual`, CLI `adminui oss-wan <cmd>,<ViewName>`, HTTP `GET /api/action/wan/state`
and `POST /api/action/wan/{ppp-connect|ppp-disconnect|dhcp-renew|dhcp-release}`.

Observed on the spare (2026-10-17; no fiber, so no WAN interface: `WANC.RealIFName` empty):
both managers reply; PPP connect/disconnect → iRet **-641** with the row in bridge mode
(ConnType 4) *and* temporarily switched to routed (ConnType 1); DHCP renew on a WANCIP row with
no WANC → **-3**, release → 0. No crash (cspd PID unchanged). -641's meaning is not decoded:
the PPP manager's handler is not in the firmware extract. A real connect/disconnect (pppd
dialing, link up/down) still needs a unit with a live PON link.

## 11. PON link events (toward a virtual WAN on a unit with no fiber)

Why the WAN never comes up on the spare: `WAND.WANDType=4` (GPON in libpmapi `tWandTypeTable`:
DSL, Ethernet, 3GDongle, MoCA, GPON, …), both `WANCD` rows `Status=2` = *Initializing*
(`tWandStatusTable`: Up, Down, Initializing, Unavailable), so no WAN connection ever gets a
`RealIFName` and the PPP manager answers -641. The PON side of this unit is **EPON** (`eoam`
daemon, `epondrv_*` kernel threads). `pon0` exists, is UP and has carrier.

Link-up path **[V: disassembly]**: `eoam` @0x51540, on EPON link up, ASENDs
`EV_XPON_LINK_UP 0xa455` (no data) to XPON_PID **0x10136** (cspd), to itself (0x850101) and
optionally to the firewall (0x10102); link down is `EV_XPON_LINK_DOWN 0xa456`. In cspd the event
table at 0x2689ec maps 0xa455/0xa456/0xa458/0xa459 to `si_xpon_link_event` (xpon_mgr.c,
@0xb5730): with `GetPonMode` & 0x34 (EPON) it calls `plib_ifs_set_phylink_dev_up("IGD.WD1")` and
ASENDs `EV_ETH_LINKUP 0xa401` to 0x101a7 (down: `_dev_down` + `EV_ETH_LINKDOWN 0xa402`).

adminui: `oss::pon_link` / CLI `adminui oss-ponlink up|down` sends exactly that ASEND.

Tried on the spare (2026-10-17): the event is delivered (kernel logs
`[old_dev_ioctl]: dev->name = nbif0, is_sfu_mode=1`), cspd and eoam stay up, but the WANCD rows
stay *Initializing*, no WAN interface is created and PPP connect still gets -641. The unit runs in
**SFU mode** (`WANC.IsSFUMode=1`; cspd has `g_port_sfu_hgu_mode` and
`EV_EPONOAM_ETH_SFU_HGU_PORT_CHANGE_CONFIG`, set from the OLT over OAM), which is the next suspect:
an SFU ONU bridges and does not run routed WAN connections. `sidbg 1 -l 8` raises cspd's log
level; its user logs did not appear in dmesg (likely the serial console).

**Update 2026-10-17 — the async flag.** The tries above sent the event with no request flag and
had no effect. liboss `ASEND` (@0x4b58) writes `event | 0x10000` at header +8 (async request;
`SSEND` uses 0x20000). With that flag, `oss-ponlink up` moves both `WANCD` rows from Status 2 to
**1**, and `oss-ponlink down` returns them to 2; cspd and eoam stay up. The SFU lead above was a
red herring: the LAN ports are all `EthPortConf.IsSfuMode=0`; only the shipped bridge WAN row has
`WANC.IsSFUMode=1`. The multi-WCD manager (`e8_multiwcd_mgr.c`, module 0x101a7) is what writes
WANCD Up: its event table (@0x2686bc) maps `0xa401`/`0xa60c` to the up handler (@0x61df8) and
`0xa402`/`0xa60d` to down; `adminui oss-ethlink up|down` sends 0xa401/0xa402 to it directly.
With the WANCD up, the WAN connection rows still have no `RealIFName` and PPP connect still
returns -641 (row is bridge mode).

**Manual connect works (2026-10-17).** `si_ppp_manual_connect` / `_disconnect` (cspd
`wancppp_mgr.c`, PPP manager event table @0x25cff0: 0x2801 -> 0x122e28, 0x2802 -> 0x1230a8)
return **-641** "WANCPPP: not found statemachine" when the connection has no PPP state machine,
**-931** when the state machine's ConnTrigger is not 2 (Manual), and -932/-933/-934 for a wrong
state (connect needs state 0 or 6). On the spare, with the row switched to routed
(`WANC.ConnType=1`, `IsSFUMode=0`), `WANCPPP.ConnTrigger=2` and `oss-ponlink up`, both
`ppp-connect` and `ppp-disconnect` returned **iRet 0**. No pppd started and the row got no
`RealIFName`: the WAN netdev is not built yet (the multi-WCD up handler only calls
`plib_ifs_register_netif` on event 0xa60c, not on 0xa401). Spare restored afterwards.
adminui now refuses PPP connect/disconnect unless the dial mode is Manual, with a reason.

**Netif step (2026-10-17), no result.** `adminui oss-ethlink netif` sends 0xa60c to the multi-WCD
manager after `oss-ponlink up`. It changed nothing visible: no new WAN netdev, `WANC.RealIFName`
still empty, connect/disconnect still iRet 0 with no pppd. `nbif0` is torn down and recreated
(new ifindex) whenever the WANC row is re-applied; it is UP while the row is routed and the link
is up, DOWN otherwise. Building the per-connection WAN device is the open problem.
