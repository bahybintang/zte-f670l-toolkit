# WLAN and Security pages: mapping notes

Covers `pages/wlan.json` (10 pages) and `pages/security.json` (8 pages).
Sources: the clone's stock UI under `debug/clone-fw/httpd/`, the decoded user DB (table and DM
names only, no values copied), `debug/tftp/pv.txt`, and the string-to-code tables in the
reference firmware's `lib/libcmapi.so` (`debug/ref-fw-v9/extracted/`).

## How the enum codes were found

The stock pages talk to `OBJ_*` objects using **strings** (`"WPAand11i"`, `"20Mhz"`, `"100%"`).
The DB stores **numeric codes** (`BeaconType=6`, `BandWidth=0`, `TxPower=1`). The conversion
lives in `libcmapi.so` `.data` as `{char *name, int code}` arrays terminated by `{0,0}`. Those
arrays were dumped from the reference firmware (genuine F670L V9). They agree with every value
in this clone's DB, for example: default 5 GHz `BandWidth=3` = 80MHz; `TxPower=1` with
`TxPowerNum=100` = 100%; `QosType=1` = WMM; `BeaconType=6` + `WPAEncryptType=2` = WPA/WPA2 TKIP+AES;
WMM `Type` 0-3 per row. The clone's own libcmapi is not in the repo, so a code it adds itself
(the 200% TX power level) cannot be checked.

| DM (table) | Stored code → meaning | Confidence |
|---|---|---|
| `BeaconType` (WLANCfg) | 0 None, 1 Basic (WEP), 2 WPA, 3 11i (WPA2), 4 BasicandWPA, 5 Basicand11i, 6 WPAand11i, 7 BasicandWPAand11i, 8 WAPIPsk, 9 WAPICert | high |
| `WEPAuthMode` | 0 None (open), 1 SharedAuthentication, 2 Both, 3 EAPAuthentication | high |
| `WPAAuthMode`, `11iAuthMode` | 0 PSKAuthentication, 1 EAPAuthentication | high |
| `WPAEncryptType`, `11iEncryptType` | 0 TKIP, 1 AES, 2 TKIP+AES | high |
| `WEPLevel` (OBJ name `WEPEncryptionLevel`) | 1 40-bit (64-bit WEP), 2 104-bit (128-bit WEP), 3 128-bit | high |
| `ACLPolicy` | 0 Disabled, 1 Ban (blacklist), 2 Allow (whitelist) | medium (table sits next to the WPS tables; owner inferred) |
| `BandWidth` (WLANBase) | 0 20MHz, 1 40MHz, 2 Auto (20/40), 3 80MHz | high |
| `SideBand` | 0 AboveControlChannel ("Upper"), 1 BelowControlChannel ("Lower") | medium: the reference table uses Above/Below, the clone UI uses Upper/Lower |
| `TxPower` | 1 100%, 2 80%, 3 60%, 4 40%, 5 20%; **0 = 200% assumed** (clone-only option, missing from the reference table) | high for 1-5, low for 0 |
| `CountryCode` | 0 CN, 1 AU, 2 BR, 3 CA, 4 EG, 5 FR, 6 DE, 7 GR, 8 HK, 9 IT, 10 KR, 11 ES, 12 GB, 13 US, 14 COUNTRY_1 ("Default setting") | high (WLANCountry has rows 0-13 with the same codes) |
| `QosType` | 0 Disabled, 1 WMM, 2 SSID | high |
| `TxRate` | 0 Auto, 1 = 1, 2 = 2, 3 = 5.5, 4 = 11, 5 = 6, 6 = 9, 7 = 12, 8 = 18, 9 = 24, 10 = 36, 11 = 48, 12 = 54 Mbps (13+ = 11n rates) | high |
| `BasicDataRates`, `OpDataRates` | 0 "1,2", 1 "1,2,5.5,11", 2 "1,2,5.5,11,6,9,12,18,24", 3 "…,36,48,54", 4 "6,9,…,54" | high |
| `WlanMode` | 0 InfrastructureAccessPoint, 1 Station, 2 Adhoc, 3 Bridge, 4 Repeater, 5 AP+STA | high (not exposed) |
| `CardMode` (capability, read only) | 1 = "b,g,n,bg,gn,bgn" (2.4G), 7 = "a,n,an,anac,ac" (5G) | high |
| `WPSMode` (WLANWPS) | 0 PushButton, 1 PIN | high (stock JS agrees) |
| WMM `Type` | 0 BE, 1 BK, 2 VI, 3 VO | high |
| `TimeSetRule` (WlanTimeCfg) | 0 EverydaySame, 1 SeparateSet | medium |
| WlanTime action | 0 TimeRFOn, 1 TimeRFOff (column name unknown) | medium |
| `MacFilterTarget`, `UrlFilterTarget` (FWBase) | 0 Discard, 1 Permit | high (stock option values for URL; firmware table for MAC) |
| MacFilter `Type` / `Protocol` | Bridge 0 / Route 1 / Bridge+Route 2; IP 0 / ARP 1 / RARP 2 / PPPoE 3 / ALL 4 | medium (table empty) |
| FWIP `Protocol` / `FilterTarget` | stock posts numbers: TCP 0, UDP 1, TCP+UDP 2, ICMP 3, ANY 4; Drop 0 / Permit 1 | medium: another firmware table has TCP 0 UDP 1 ICMP 2 ANY 3 |
| FWLevel `Level` | 1 Low, 2 Middle (hidden), 3 High | high (stock option values) |
| FWSC `IPMode` | 1 IPv4, 2 IPv6 | high (stock JS) |
| FWSC `Servise` | bitmask HTTP 1, FTP 2, SSH 4, TELNET 8, HTTPS 16, SNMP 32 | high (stock JS) |

### Wireless mode is derived, not stored

Stock "Mode" (OBJ param `Standard`, codes a 1, b 2, b,g 3, n 4, g 5, g,n 6, b,g,n 7, a,n 8,
a,n,ac 9, ac 10) has no DM. The stock JS writes these fields together:

| Stock mode | BasicDataRates | OpDataRates | 11nMode | GreenField | 11acMode |
|---|---|---|---|---|---|
| 802.11b only | 0 | 1 | 0 | – | – |
| 802.11g only | 2 | 3 | 0 | – | – |
| b+g | 1 | 3 | 0 | – | – |
| 802.11n only | 1 | 3 | 1 | 1 | – |
| g+n | 2 | 3 | 1 | 0 | – |
| b+g+n (2.4G default) | 1 | 3 | 1 | 0 | – |
| a+n | – | – | 1 | 0 | 0 |
| a+n+ac (5G default) | – | – | 1 | 0 | 1 |

The schema exposes the individual DMs (rate sets marked advanced). A single "Wireless mode"
dropdown would need a composite field type (one option writing several DMs). Worth adding to
SCHEMA.md.

## Row mapping

- WLANCfg, WLANPSK, WLANWPS: row = SSID index 0-7 (rows 0-3 = wlan0-3 on 2.4 GHz, rows 4-7 = wlan4-7 on 5 GHz).
- WLANBase: row 0 = 2.4 GHz (wlan0), row 1 = 5 GHz (wlan4). `wlan-radio` uses a **radio** selector
  (values 0/1) with one section per band (`rowMap {"0":0}` / `{"1":1}`), because the enums differ
  per band (channels, 80 MHz). `wlan-wps` uses an SSID selector (0, 4) with `rowMap {"0":0,"4":1}`.
- WLANWEP: 32 rows, `IGD.LD1.WLAN<n>.WEP<k>` → row = 4·ssid + (k−1). `wlan-security` has four sections
  `wep1`-`wep4`, each `rowMap {ssid: 4·ssid + k−1}`.
- WLANWMM: 8 rows = WLAN1.WMM1-4 (rows 0-3, 2.4 GHz) and WLAN5.WMM1-4 (rows 4-7, 5 GHz). One section per AC with `rowMap {"0": t, "1": 4+t}`.
- WLANROAMING: 2 rows; I guessed row 0 = 2.4 GHz, row 1 = 5 GHz.
- Stock pages hide any SSID whose `LocalSetEnable` is not 1 (all 8 are 1 on this device).

## Live-apply key map (hook `wifi`)

`hooks.rs` sends `iwpriv <iface> set <liveKey>=<new DB value>` **without translation**, and maps
WLANBase row 0/1 → wlan0/wlan4 and any other table's row r → `wlan{r}`. So a liveKey is only set
where the stored value is exactly what the driver takes, and only on tables where row r means wlan{r}.

| DM | liveKey | Confidence |
|---|---|---|
| WLANCfg `ESSID` | `SSID` | verified on device |
| WLANCfg `ESSIDHideEnable` | `HideSSID` | verified on device |
| WLANPSK `KeyPassphrase` | `WPAPSK` | verified on device |
| WLANCfg `Enable` | (none; the hook runs `ifconfig wlanN up/down`) | verified on device |
| WLANCfg `VapIsolationEnable` | `NoForwarding` | medium (standard MediaTek AP key) |
| WLANCfg `MaxUserNum` | `MaxStaNum` | medium |
| WLANBase `RadioStatus` | `RadioOn` | medium |
| WLANBase `Channel` | `Channel` | high (sent only when the channel value changes) |
| WLANBase `BeaconInterval` | `BeaconPeriod` | medium-high |
| WLANBase `DTIM` | `DtimPeriod` | medium-high |
| WLANBase `RtsCts` | `RTSThreshold` | medium-high |
| WLANBase `Fragment` | `FragThreshold` | medium-high |
| WLANBase `SGIEnabled` | `HtGi` | medium |
| WLANBase `SSIDIsolationEnable` | `NoForwardingBTNBSSID` | medium |

**No liveKey: these need a translating hook.** Each takes effect after a reboot until the hook translates it.

| DM | iwpriv key | Translation needed |
|---|---|---|
| `BeaconType` + `WEPAuthMode` + `*AuthMode` | `AuthMode` | None/Basic+None → OPEN; Basic+Shared → SHARED; Basic+Both → WEPAUTO; WPA → WPAPSK; 11i → WPA2PSK; WPAand11i → WPAPSKWPA2PSK |
| `*EncryptType` / BeaconType | `EncrypType` | None → NONE; Basic → WEP; 0 → TKIP; 1 → AES; 2 → TKIPAES |
| `BandWidth` | `HtBw` (+ `VHT_BW`) | 0 → HtBw=0; 1/2 → HtBw=1 (VHT_BW=0); 3 → HtBw=1 + VHT_BW=1 |
| `SideBand` | `HtExtcha` | 0 (Upper/above) → 1, 1 (Lower/below) → 0 (needs checking) |
| `TxPower` | `TxPower` | code → percent (1 → 100 … 5 → 20); 200% has no driver equivalent |
| mode DMs | `WirelessMode` | b+g+n → 9, a+n+ac → 14, etc. (MediaTek PHY mode codes) |
| `AutoChannelEnabled` | `AutoChannelSel` | 1 → 1 or 2 (algorithm); 0 → set an explicit `Channel` |
| `CountryCode` | `CountryRegion` / `CountryRegionABand` | ZTE code → MediaTek region index |
| `WEPKeyIndex`, WLANWEP `Key` | `DefaultKeyID`, `Key1`-`Key4` | values match, but WLANWEP rows ≠ interfaces (row 4 is SSID2 key 1, not wlan4). Map iface = row/4, key = row%4+1. |
| `WPAGroupRekey` | `RekeyInterval` (+ `RekeyMethod=TIME`) | probable 1:1, untested |
| WLANWPS `Enable`/`WPSMode` | `WscConfMode` / `WscMode` | Enable → WscConfMode 7/0; PBC 0 → WscMode 2, PIN 1 → WscMode 1 |
| WLANWMM | `APAifsn`, `APCwmin`, `APCwmax`, `APTxop` | semicolon list of 4 ACs per radio; WMM rows ≠ interfaces |

The hook already orders AuthMode → EncrypType → WPAPSK → SSID, so translated security keys can slot in.

## Hidden-but-present tables mapped (stock menu has none)

All of these carry `"stock": {"menu": null, …}`.

- `wlan-mac-acl`: WLANCfg `ACLPolicy` and `MACAddressControlEnabled` (known DMs). WLANACLWHITE and
  WLANACLBLACK (pv max 40 rows each) are **empty**, so their DM names are unknown; the sections
  are read only with no fields. To finish: add one entry (a TR-069 or `sidbg DB add` test) and
  read the table, or pull the clone's `/lib/libcmapi.so` / `cspd` and dump their DM tables.
- `wlan-steering`: WLANBANDSTEERING (row 0), WLANBase `BandSteer` per radio, WLANROAMING rows 0-1.
  Ranges are assumptions. The firmware gives no JS limits.
- `wlan-schedule`: WlanTimeCfg (TimerEnable, SetEnable, TimeSetRule, SSIDMask). WlanTime is empty
  (DM names unknown; the reference cspd defaults `IGD.LD1.WlanTime1/2` with `Hour`/`Minute` and
  refuses on/off gaps under 10 minutes). The separate CMCC "Wi-Fi sleep" timer
  (`OBJ_WIFI_SLEEP_ID`, `common_page/aset_wifiTimerSetting.lp`) probably lives in SLWTIMEINFO; not mapped.
- `wlan-wmm`: WLANWMM (all DMs known).
- `wlan-mesh`: EASYMESHCONFIG, read only (role and medium codes undocumented).
- Not mapped: WIFISHARE (has ShareAccount, possibly personal), WLANCfg5GBak (the firmware's
  internal backup of 5 GHz SSIDs), WLANCountry (lookup table), WLANAD (associated devices: runtime).

## Security mapping

| Page | OBJ | Table(s) | Notes |
|---|---|---|---|
| sec-url-filter | OBJ_FWBASE_ID, OBJ_FWURL_ID | FWBase, FWURL | FWURL is empty, so its DMs (`Url`, `Enable`, `FilterTarget`) come from the OBJ list |
| sec-firewall-level | OBJ_FWLEVEL_ID | FWLevel | `Level`; 3 extra DMs added as advanced |
| sec-firewall-attack | OBJ_FWLEVEL_ID | FWLevel | `AntiAttack` |
| sec-mac-filter | OBJ_FWBASE_ID, OBJ_MACFILTER_ID | FWBase, MacFilter | MacFilter is empty, so its DMs are unverified |
| sec-ip-filter | OBJ_FWIP_ID | FWIP | FWIP is empty, so its 22 DMs are unverified; ports −1 = any |
| sec-service-control | OBJ_FWSC_ID | FWSC (+FWBase) | DMs verified (one stored rule); `Servise` really is spelled that way |
| sec-service-ports | OBJ_FW_SERVPORT_ID | PortControl | OBJ `ServPort` → DM `PortValue` is inferred |
| sec-telnet | OBJ_TSERVER_CONF_ID | TelnetCfg | stock handler calls `undoDBSave()`: stock changes are runtime-only |

Danger levels: every firewall page, MAC/IP filter, service control, remote ports and telnet are
`commit-confirm`; the URL filter is `confirm`. `apply` is `unknown` for every security page: the
stock path notifies the firewall module through `set_inst`, while a raw `sidbg DB set` + save
probably needs a reboot or a firewall reload. Check one page on the device before trusting it.

## Uncertainties, ranked

1. **Empty tables**: FWURL, MacFilter, FWIP, WLANACLWHITE/BLACK and WlanTime have no rows, so the
   DM names are the OBJ parameter names (or unknown) and the stored code format is assumed.
   Create one row through the stock UI, then run `sidbg 1 DB p <Table>` before enabling writes.
2. **Security apply path**: does `sidbg set` + `save` change the running iptables without a reboot?
3. **Telnet persistence**: writing TelnetCfg persists a setting that the stock UI deliberately keeps
   runtime-only. `TS_Enable=0` saved means no telnet after a reboot.
4. `TxPower` code 0 = 200% (clone-only option) is assumed. `SideBand` 0 = Upper is inferred.
5. FWIP `Protocol` codes 2-4 (two conflicting firmware tables).
6. The medium-confidence liveKeys (RadioOn, NoForwarding, MaxStaNum, HtGi, NoForwardingBTNBSSID,
   BeaconPeriod/DtimPeriod/RTSThreshold/FragThreshold) are standard MediaTek keys but untested on this
   RTPRIV build. Run `iwpriv wlan0 set <Key>=<v>` and check `iwpriv wlan0 show` or `dmesg`.
7. WLANROAMING row-to-band, WlanTimeCfg `SSIDMask`/`SetEnable` meaning, EasyMesh codes.
8. The schema cannot express cross-field validation (start ≤ end ranges, prefix+SSID ≤ 32,
   WPS requires WPA2), cross-section showIf (PSK/WEP sections show for every BeaconType),
   bitmask checkbox groups (`Servise`), or runtime option lists (`INCViewName`). These are
   candidate schema extensions.
