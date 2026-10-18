# adminui page schema

Every admin page is data, not code. `pages/*.json` files are embedded into the
binary (server-side validation + write whitelist) and imported by the web app
(form rendering + client-side validation). One source of truth for both.

Each file is `{ "pages": [Page, ...] }`.

## Page

```jsonc
{
  "id": "wlan-ssids",                 // unique, kebab-case, stable (URL slug)
  "title": "Wi-Fi networks",          // English, human (not the stock label verbatim if it is bad English)
  "group": "wlan",                    // status | network | wlan | security | application | admin | diagnostics
  "order": 20,                        // sort within group
  "icon": "wifi",                     // lucide icon name (optional)
  "description": "One sentence shown under the title.",
  "stock": { "menu": "ssmWLANMul", "page": "net_wlan_essid_t.gch", "handler": ["wlan_essid.gch"] },
  "applicable": true,                 // true | false | "unknown" — does this page apply to this GPON F670L?
  "applicableNote": "",               // why false/unknown (e.g. "ADSL page; device is GPON")
  "apply": "live",                    // live | save | reboot | unknown  — how a write takes effect (best guess; verified later)
  "danger": "none",                   // none | confirm | commit-confirm (WAN/LAN-IP/firewall => commit-confirm)
  "hook": "wifi",                     // optional: named server-side live-apply hook (only "wifi" exists today)
  "selector": {                       // optional: page-level instance picker; its value is referenced as "$sel"
    "label": "Network",
    "options": [ { "value": "0", "label": "SSID1 · 2.4 GHz" } ]
  },
  "sections": [ Section, ... ],
  "actions": [ Action, ... ],          // optional buttons (reboot, ping, ...)
  "notes": "free text: mapping doubts, stock behaviour, OBJ names that are runtime-only"
}
```

## Section

A section reads/writes exactly **one DB table** (as named by `sidbg 1 DB p <Table>`,
i.e. the `<Tbl name=...>` names in the user config, e.g. `WLANCfg`, `WLANBase`,
`WLANPSK`, `DHCPSHostCfg`). If a stock page spans several tables, make one section per table.

```jsonc
{
  "id": "basic",
  "title": "Basic",
  "table": "WLANCfg",
  "mode": "single",          // single  = edit one row
                             // list    = show all rows as a table; add/edit/delete if allowed
                             // readonly= display only (status)
  "row": "$sel",             // single mode: a row number (0) or "$sel" (selector value)
  "rowMap": { "4": 1 },      // optional: map selector value -> this table's row (e.g. SSID row -> radio row)
                             //   keys missing from rowMap => section hidden for that selection
  "rowFilter": { "dm": "Enable", "in": ["1"] },   // optional (list mode)
  "rowLabel": "Rule {n}",    // optional (list mode), {n}=row no, {DMName}=field value
  "addable": false, "deletable": false,  // list mode
  "maxRows": 32,             // list mode, from the stock page/js limit if known
  "source": "db",            // db (default) | runtime:<name>  (runtime = not in the DB; server computes it, see below)
  "fields": [ Field, ... ]
}
```

## Field

```jsonc
{
  "dm": "ESSID",             // exact DM name in the table
  "label": "Network name (SSID)",
  "type": "string",          // string | int | bool | enum | ipv4 | ipv6 | mac | password | text(multiline) | hex
  "min": 1, "max": 32,       // string/password: length; int: value
  "pattern": "^[^'\"]*$",    // optional JS+Rust compatible regex (no lookaround)
  "on": "1", "off": "0",     // bool: the stored values (default "1"/"0")
  "options": [ { "value": "0", "label": "Auto" } ],   // enum
  "unit": "dBm",
  "placeholder": "",
  "help": "Short help, English",
  "readonly": false,
  "advanced": false,         // hidden behind an "Advanced" toggle
  "column": true,            // list mode: show as a table column
  "required": false,
  "showIf": { "dm": "BeaconType", "in": ["WPAand11i", "11i"] },   // same-section dependency
  "default": "",             // list mode: value for new rows (only if stock js sets one; never personal data)
  "liveKey": "SSID"          // optional: for hook=wifi, the iwpriv key that applies this DM live
}
```

`password` fields are **write-only**: the server never returns their values.

## Action

```jsonc
{ "id": "reboot", "label": "Restart device", "danger": "confirm",
  "kind": "builtin",         // builtin = implemented in Rust under this id
  "note": "how the stock page does it (DB table writes? special cgi? command?)" }
```

## Runtime sources

Status pages often read objects that are not stored DB tables (live counters,
PON optics, link state). Use `"source": "runtime:<name>"` with a descriptive
name (`runtime:pon_optics`, `runtime:wan_status`, `runtime:eth_ports`,
`runtime:wlan_clients`, ...) and list the fields the stock page shows (dm = a
short key, label = English). In `notes`, say which OBJ name / gch the stock page
used so the server implementer can find the data (proc, sysfs, iwpriv, sidbg).

## Rules

- No personal data in schema files: no real SSIDs, passwords, MACs, serials,
  LOIDs, account names, ISP URLs. Defaults only if they are firmware defaults.
- English labels: take them from `langen.conf` (`&?key;` tokens) and fix the English.
- Options/enums: copy the exact stored values from the stock JS/gch; labels in English.
- Validation: copy limits from the stock JS (length, ranges, char rules).
