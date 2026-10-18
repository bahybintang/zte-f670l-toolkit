# Status, Diagnosis and Help: mapping notes

Sources: `debug/clone-fw/httpd/` (dmenu.conf, langen.conf, `*_t.gch`, `*_gch.gch`, `*_js.gch`),
`debug/decoded/db_user_cfg.plaintext.xml` (table/DM names only), `debug/tftp/pv.txt`,
and `debug/ref-fw-v9/.../webmodules/modules/networkdiag_*_lua.lua` for the ping cross-check.
Schema files: `pages/status.json` (18 pages) and `pages/diagnostics.json` (12 pages, including Help).

General findings

- Almost every status value is runtime. The stock pages read `OBJ_*` objects through cspd. Those
  objects do not exist as DB tables, so `sidbg 1 DB p` cannot return them. The DB holds the config
  side only: WANC/WANCIP/WANCPPP, WLANCfg/WLANBase, LAND, EthPortConf, VoIPSIPLine.
- Where a page mixes the two, the schema shows a read-only `db` section (config) next to a
  `runtime:` section (live values).
- The diagnostics handlers (ping, traceroute, inform, mirror, simulation) all end with
  `undoDBSave()`. Stock runs a diagnostic without persisting it to flash. The new backend should
  run `ping`/`traceroute` itself through a builtin action, not through the DB.
- `TypeValue=2` (Ethernet/PON uplink) selects the `OBJ_ETHWANC*` objects. The `OBJ_DSLWANC*`
  branches are dead on this device.
- Interface facts I verified in the DB: EthPortConf rows 0-3 have IFName `eth0`-`eth3` (views
  IGD.LD1.ETH1-4). WLANBase row 0 has Band=0 (2.4 GHz) and row 1 has Band=1 (5 GHz). Wi-Fi uses
  `wlan0-3` / `wlan4-7` for WLANCfg rows 0-3 / 4-7.

## Runtime sources (name -> contents -> stock OBJ -> where to look on the device)

| runtime: | Fields | Stock OBJ | Device candidates (unverified) |
|---|---|---|---|
| device_info | ModelName, SerialNumber, ManuFacturerOui, HardwareVer, SoftwareVer, BootVer, SoftwareVerExtent, DeviceType, AccessType, PonMode | OBJ_DEVINFO_ID; env `ponmode` | board/flash info, /etc version files; DevInfo table has DeviceType/AccessType only |
| pon_link | LoidState, FecEnable, LosInfo | OBJ_PON_LOID_ID, OBJ_GPON_FECSTATUS_ID, OBJ_GPON_LOS_INFO_ID | cspd PON module (`sidbg 1 -p`), gpon_omci |
| pon_optics | RxPower, TxPower (raw/10000 dBm), Volt (raw mV), Current (uA), Temp (raw/1000 C) | OBJ_PON_OPTICALPARA_ID | optic/BOSA driver /proc entry |
| pon_gemports | EncrypMode, Rx/TxgemFrames, Rx/TxgemOctects (list) | OBJ_PON_GEMPORTSTAT_ID | PON driver |
| pon_counters | FECErrors, DropPackets, Packet*/Uni*/Mul*/Broad* Sent/Recv | OBJ_PON_STAT_ID | PON driver (may be EPON only) |
| alarms | LosInfo, VoiceAlarm (derived) + three always-blank rows | OBJ_GPON_LOS_INFO_ID, OBJ_VOIPVPLINE_ID | derived from pon_link + voip_lines |
| wan_ipv4 | WANCName, Type, IsNAT, IPAddress, SubnetMask, GateWay, DNS1-3, WorkIFMac, ConnStatus, ConnError, UpTime, RemainLeaseTime | OBJ_ETHWANCPPP_ID / OBJ_ETHWANCIP_ID / OBJ_ETHWANCBRIDGE_ID | `ip -4 addr/route`, pppd/udhcpc state |
| wan_ipv6 | GuaCurSrc, GUA1-3 (+IsOutPreferredLft1-3 as prefix len), GuaNum, DNSv6CurSrc, DNS1v6-3v6, PrefixCurSrc, Prefix1/Prefix1Len, GuaPd, Gatewayv6CurSrc, Gatewayv6, ConnStatusV6 | same OBJs, IPMode 2/3 only | `ip -6 addr/route`, DHCPv6 client state |
| dslite_tunnels | Tunnel46name, Tunneltype (8=ds-lite), WANCName, TUN46Localv4Addr, AFTRAddr, ConnStatus (0 offline) | OBJ_TUNNEL_ID + OBJ_TUNNEL46_ID | `ip -6 tunnel` |
| lan_bridge | IPAddr, IPv6Addr, IPv6PrefLen | OBJ_BRGRP_ID | `ip addr show br0` (LAND has configured IPAddr) |
| lan_ports | Status, Speed, Duplex, MACAddress, In/OutOctets, In/OutPkts, CRC | OBJ_PON_ETH_ID | /sys/class/net/ethN/{carrier,speed,duplex,statistics} |
| dhcp_leases | HostName, MACAddr, IPAddr, ExpiredTime (-1 infinite), PhyPortName, PhyType | OBJ_DHCPHOSTINFO_ID, OBJ_DHCP6C_ID, OBJ_MACTOTYPE_ID | DHCP server lease file (path to find), `ip neigh` |
| wlan_ssid_stats | Index, RealRF, ChannelInUsed, Bssid, TotalBytes/Packets Received/Sent, Errors*, Discard* | OBJ_WLANCONFIGDRV_ID (IGD.LD1.WLANn) | /sys/class/net/wlanN/statistics, `iwconfig`, `iwpriv wlanN stat` |
| usb_devices | PanelPort, DevName, ClassInfoStr, DevID | OBJ_USBDEV_ID | /sys/bus/usb/devices |
| voip_lines | Line, IsOnline | OBJ_VOIPVPLINE_ID | voice daemon state |
| tr069_status | InfoStat, InfoDesc, AcsStat, AcsDesc | OBJ_tr069Stat_ID / OBJ_tr069InfmStat_ID | cwmp client state |
| service_config | Result (99/0/1/2) | OBJ_ServQry_ID | maybe DB PDTCTUSERINFO.Result |
| smart_gateway | distribute*, registercenter*, plugincenter* | OBJ_GatewayNew_ID | n/a (not applicable) |
| arp_table | DestIP, MACAddr, Interface | OBJ_GETARPNUM_ID + OBJ_GETARPINST_ID | /proc/net/arp |
| mac_table | LanName, MACAddr, AgingTm | OBJ_GETMACNUM_ID + OBJ_GETMACINST_ID | `brctl showmacs br0` |
| ping_request / ping | Host, Interface, NumofRepeat, DataBlockSize, Timeout / DiagnosticsState, PingAck, Success/FailureCount, Min/Avg/MaxResponseTime | OBJ_DEVPING_ID (= PINGDiag row 0, view IGD.PINGDIAG.CFG1) | busybox ping |
| traceroute_request / traceroute | Host, Interface, MaxHopCount, Timeout, Protocol / Flag, Result (file path) | OBJ_TRACERT_ID (no table) | busybox traceroute |
| simulation_request / simulation | PPPOE_* / DHCP_* params / PortState, Issucceed, SimuIp, SimuGateWay, SessionID, FailReason, Ping* | OBJ_SIMULATION_{PPPOE,DHCP}[_GET]_ID | cspd only |
| voice_line_test, voice_call_request, voice_call_test | TestStatus, vac/vdc/res/cap_{tr,tg,rg}, ring_vol, ren, loop_*, battary; DialNumber; CTCallerTestResult | OBJ_VOIP_DIAGNOSIS_ID, OBJ_VOIP_CALL_ID | cspd/SLIC only |
| atm_oam | Destination_address | OBJ_ATM_ID | n/a (DSL) |

## Status pages

- **status-device** (ssmDevStatu, status_dev_info_t.gch): device_info only. The page shows the
  GPON serial as `ManuFacturerOui-SerialNumber`. BootVer and batch number are hidden in stock.
  `status_dev_info_gch.gch` (Fujian variant) dumps WLAN PSKs as hidden fields; do not copy it.
- **status-wan-ipv4** (ssmETHWANConn): read-only `WANC` section plus wan_ipv4. Stock hides rows with
  IPMode=2. The PPP status enum has 7 values. ConnError uses `ERROR_*` codes. Open question: how a
  WANC row maps to a Linux ifname (WorkIFMac may help).
- **status-wan-ipv6** (ssmIPv6WANSta): wan_ipv6 for IPMode 2/3. The source enums are AutoConfigured,
  RouterAdvertisement, DHCPv6/PrefixDelegation, Static, PPP, PPPIPV6CP and AutoAdaptive. Stock
  labels IsOutPreferredLftN as a prefix length; that is a stock quirk.
- **status-dslite** (ssmDSLITEStatus): `applicable:false` because it is the DSL uplink variant.
- **status-dslite-eth** (ssmDSLITEEthSta): `unknown`. TUNNEL46CFG has 0 rows.
- **status-epon-link** (ssmLinkState): `false` (EPON).
- **status-pon** (ssmGemStat, gpon_status_link_t.gch): pon_link + pon_optics + pon_gemports.
  - The LoidState labels depend on env RegisterPageLangType/CountryCode. I used the default set
    (0/5 init, 1 ok, 2 LOID wrong, 3 password wrong, 4 conflict); the CountryCode on this unit is
    unknown.
  - Stock hides the optics unless the user is admin (Right==1), and hides the GEM tables always.
  - The stock "GPON state" is the LOID auth state, not O1-O5.
  - Never return PonLoid or LoidPasswd on this page.
  - gpon_status_link_info_t(_user).gch and pon_status_gemport_info_t.gch are covered here.
- **status-pon-counters** (off-menu pon_status_stat_info_t.gch): `unknown` in GPON.
- **status-alarms** (ssmAlarm): only two alarms have real data: LOS and "no VoIP line registered".
  CATV is dropped (IsOnuCatv).
- **status-wlan** (Menu3_SL_WLANinfo, status_wlaninfo_t.gch): `WLANBase` + `WLANCfg` read-only DB
  sections plus wlan_ssid_stats.
  - Stock lists only SSIDs with LocalSetEnable=1 (`rowFilter`).
  - Its "connection status" column is the radio's RadioStatus.
  - e8_status_wlan_info_t.gch is folded in (BSSID, error and discard counters).
  - No stock status page lists associated clients.
- **status-ethernet** (Menu3_SL_Ethinfo, status_ponlan_info_t.gch): lan_bridge + lan_ports. This
  merges status_lan_info_t_user.gch / gpon_status_lan_info_t.gch (MAC, CRC, bridge IPs).
  - Status codes conflict between the two stock pages: user page = 0 Up, 1 NoLink, 3 Disabled,
    else Error; ponlan page = 0 connected, else disconnected.
  - Speed 0/1/2 = 10/100/1000; Duplex 0 half, 1 full.
  - The stock CT build renames LAN2 to "iTV"; I did not copy that.
- **status-lan-clients** (off-menu status_ethlan_dhcp_info_t.gch): dhcp_leases. The same table is at
  the top of the Ethernet page. PhyType codes are 0-9. Stock prints the wrong label ("DNS Server2")
  for ExpiredTime -1.
- **status-usb** (ssmUSB): usb_devices.
- **status-tr069** (ssmItms, admin): tr069_status. InfoStat/InfoDesc/AcsStat code tables are in the
  JSON.
- **status-service-config** (ssmServ, admin): service_config, Result 99/0/1/2. The DB backing is a
  guess.
- **status-voip** (ssmVOIPStatu) and **status-voip-accounts** (ssmPhoneNumber): voip_lines plus
  read-only `VoIPSIPLine` (AuthUserName, else DigestUserName). Both are `unknown` (POTS use).
- **status-smart-gateway** (smSmartStatu has no leaf): `false`. This covers status_interactive_t,
  status_plugin_t and status_reboot_framework_t.

## Diagnostics pages

- **diag-line** (ssmLineDiag, ATM OAM loopback): `false`.
- **diag-ping** (ssmPingDiag): stock `set_inst(OBJ_DEVPING_ID)` with Host and Interface, plus fixed
  values DiagnosticsState=Requested, NumofRepeat=4, DataBlockSize=56, Timeout=2000. Stock does not
  save to flash. It polls `PingAck` every 3 s for at most 8 tries.
  - The DB table is PINGDiag row 0. The OBJ param `NumofRepeat` maps to DM `NumberOfRepetitions`,
    and DiagnosticsState is stored as a number.
  - Rows 1-3 belong to other users; do not touch them.
  - Builtin action `ping` is recommended.
- **diag-traceroute** (ssmTraceRoute): OBJ_TRACERT_ID, which has no table.
  - Start sets Control=0 and DiagnosticsState=Requested. Stop sets Control=1.
  - Flag=1 means running. The page polls every 3 s, for at most 500 tries.
  - `Result` is a file path that the page reads.
  - Limits: hops 1-64, wait 1-4294967295 (the unit is not stated; TR-098 uses ms). Protocol is UDP
    or ICMP.
  - Builtin actions are `traceroute` and `traceroute-stop`.
- **diag-arp** (ssmARPTable) and **diag-mac-table** (off-menu): count + threshold objects, then the
  instance objects. /proc/net/arp and `brctl showmacs` are enough.
- **diag-tr069-inform** (ssmInform, admin): `set_inst(OBJ_tr069InfmAct_ID, "IGD.MGT")` with
  EventCode `2 PERIODIC`. It polls OBJ_tr069InfmStat_ID 40 times at 3 s intervals. It contacts the
  ISP ACS, so the action has `danger:confirm`.
- **diag-simulation** (ssmStimulate): PPPoE/IPoE emulation through the OBJ_SIMULATION_* objects and
  ModuleLogic. `unknown`.
  - Validation: user name 1-64 printable ASCII, password 0-64.
  - The stock JS echoes the password into the result text; do not copy that.
  - Running it with the live PPPoE account may drop the real session.
- **diag-voice** (ssmDSVoice): OBJ_VOIP_DIAGNOSIS_ID line test and OBJ_VOIP_CALL_ID simulated call.
  `unknown`.
  - Stock submits the 8 command params (port, item, obligate, port_type, num, omci_item, cmd, flg)
    empty, and the values cspd expects are unknown.
  - The CTCallerTestResult enum is taken from the VoiceState[] index.
- **diag-mirror** (ssmMirro): `MirrorConf` list (MirrorEnable, MirrorSrc, MirrorDest).
  - **The DM names are unverified** because the table has 0 rows. View 342 `MirrorRule` is another
    candidate.
  - Sources are WAN view names (IGD.WD1.WCD1 = all). Destinations are OBJ_WANLAN_ID entries with
    BRType=0 and WANLANType=2.
  - I could not determine how a change takes effect, so `apply` is `unknown`.
- **diag-monitor** (smmonitorconfig): OBJ_PONMONITORCONFIG_ID mapped to `MonitorCollectorCfg`
  (Enable, ServerUrl, TftpPort) by matching names; this is unverified. Stock posts only Enable, and
  only while the setting is already enabled. `unknown`.
- **diag-monitor-anhui**: `false` (province page, same handler).
- **help** (ssmHelp, group admin): help_t.gch renders the nameTag/helpTag of every dmenu entry. The
  page has no data.

## Top uncertainties

1. There is no on-device source yet for PON optics, LOID state, GEM, VoIP registration and TR-069
   state. They need RE of cspd/sidbg or the driver /proc entries.
2. WANC row -> Linux ifname mapping for the WAN status pages.
3. MirrorConf DM names, and MonitorCollectorCfg as the backing table for OBJ_PONMONITORCONFIG_ID.
4. The ping OBJ->DB mapping (NumofRepeat vs NumberOfRepetitions, numeric DiagnosticsState). Running
   busybox ping/traceroute directly avoids the question.
5. Which LoidState label set applies (depends on CountryCode), and the exact raw units of optics.
