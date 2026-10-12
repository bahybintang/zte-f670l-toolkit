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
| 0x0E | 2 | seq/inst (`*seq`) | request sequence (in/out) |
| 0x10 | 2 | copy of `*seq` | also the expected-reply-length slot after return |
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
