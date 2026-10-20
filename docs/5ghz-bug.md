# Why the stock UI cannot save 5 GHz settings

**Symptom.** Any change to a 5 GHz network in the stock web UI (SSID, password, channel, anything)
fails with:

```
Web set failed.(objname: OBJ_WLANCONFIG_ID identity: IGD.LD1.WLAN5 iRet: -258)
```

The 5 GHz radio itself works and broadcasts. Only saving fails. 2.4 GHz saves succeed. A factory
reset does not help.

## Root cause

The firmware's processes talk over an internal message bus ("OSS", see
[oss-protocol.md](../adminui/notes/oss-protocol.md)), and every module has a 32-bit address.
When you save a 5 GHz setting, the configuration daemon (`cspd`) sends a second, per-radio
message to a module that **this firmware build does not contain**. The router's own log
(`/var/log/tmplog1`) records it:

```
From thread[cspd_task] not Find Dest PID  Sender:[00011701]  Receiver:[0001014B]  Event=0xae09
Web set failed.(objname: OBJ_WLANCONFIG_ID identity: IGD.LD1.WLAN5 iRet: -258)
```

- The main Wi-Fi apply goes to `wlan_mgr` (`0x00011201`), which is alive and works for both bands.
- The failing message is sent to `0x0001014B` (module ID `0x4B`). cspd's static table of 56
  modules has no entry with that ID, and neither the address nor event `0xae09` appears as a
  constant in any firmware binary or library. Both are computed at run time, per radio.
- The Wi-Fi driver layer agrees with this. The pre-built interface table in `libpmapi`
  (`g_twlan_wlan_if_table`) lists only `wlan0`–`wlan3`, the 2.4 GHz interfaces, while the
  5 GHz interfaces on this board are `wlan4`–`wlan7`.

So it is a mismatch between this firmware build and the dual-band hardware. It isn't caused by
bad settings or by the web page's JavaScript; submitting valid values still fails at the same
point.

## Why there is no stock-UI fix

- The message path is computed at run time and spread across `cspd` and shared libraries.
  Bounded reverse engineering found no small, safe binary patch.
- No firmware image for this clone is available to compare against or reflash, and a failed
  patch to `cspd` can take down the whole management plane.

## What works instead

- **adminui** writes the settings to the config database and applies them with the firmware's
  own change notification plus `iwpriv`. 5 GHz changes take effect live. See
  [adminui/README.md](../adminui/README.md).
- **The config-file route**: edit `/userconfig/cfg/db_user_cfg.xml` with the `zte670`
  toolkit, copy it back and restart. See [toolkit.md](toolkit.md#change-5-ghz-through-the-config-file).
