//! Runtime (non-DB) data sources referenced by schema sections as
//! `"source": "runtime:<name>"`, and the front-panel LED summary.
//! Sources are the firmware's own debug tools, cached a few seconds because
//! each call opens a driver device.

use crate::state::App;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const T: Duration = Duration::from_secs(10);
const TTL: Duration = Duration::from_secs(3);

fn cache() -> &'static Mutex<HashMap<String, (Instant, Value)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Value)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

async fn cached<F, Fut>(key: &str, f: F) -> Result<Value, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Value, String>>,
{
    if let Some((t, v)) = cache().lock().unwrap().get(key) {
        if t.elapsed() < TTL {
            return Ok(v.clone());
        }
    }
    let v = f().await?;
    cache().lock().unwrap().insert(key.to_string(), (Instant::now(), v.clone()));
    Ok(v)
}

pub async fn read(app: &App, name: &str) -> Result<Value, String> {
    match name {
        "system" => Ok(serde_json::to_value(crate::status::system()).unwrap()),
        "arp" | "arp_table" => Ok(json!({ "rows": crate::status::arp() })),
        "traffic" => Ok(serde_json::to_value(app.stats.traffic()).unwrap()),
        "pon_optics" => cached(name, || optics(app)).await,
        "lan_ports" | "eth_ports" => cached("lan_ports", || lan_ports(app)).await,
        "hosts" | "dhcp_leases" => cached("hosts", || hosts(app)).await,
        "device_info" => device_info(app).await,
        other => Err(format!("live data '{other}' is not available yet")),
    }
}

/// `key = value` / `key: value` lines -> map.
pub fn kv_lines(s: &str) -> Map<String, Value> {
    let mut m = Map::new();
    for l in s.lines() {
        let l = l.trim();
        let Some((k, v)) = l.split_once('=').or_else(|| l.split_once(':')) else { continue };
        let k = k.trim().trim_start_matches("optical ").to_string();
        if !k.is_empty() && !k.contains(' ') {
            m.insert(k, json!(v.trim()));
        }
    }
    m
}

fn num(m: &Map<String, Value>, k: &str) -> Option<f64> {
    m.get(k)?.as_str()?.split_whitespace().next()?.parse().ok()
}

/// SFF-8472 units: temp 1/256 °C, voltage 100 µV, bias 2 µA, power 0.1 µW.
pub fn optics_from(raw: &str) -> Value {
    let m = kv_lines(raw);
    let dbm = |uw10: f64| if uw10 <= 0.0 { None } else { Some(10.0 * (uw10 / 10000.0).log10()) };
    let rx = num(&m, "RXPower").unwrap_or(0.0);
    let tx = num(&m, "TXPower").unwrap_or(0.0);
    json!({
        "temperatureC": num(&m, "temp").map(|t| (t / 256.0 * 10.0).round() / 10.0),
        "supplyVoltageV": num(&m, "SupplyVoltage").map(|v| (v / 10000.0 * 100.0).round() / 100.0),
        "biasCurrentMa": num(&m, "TXBiasCurrent").map(|v| (v * 2.0 / 1000.0 * 10.0).round() / 10.0),
        "txPowerDbm": dbm(tx).map(|v| (v * 100.0).round() / 100.0),
        "rxPowerDbm": dbm(rx).map(|v| (v * 100.0).round() / 100.0),
        "los": rx <= 0.0,
    })
}

async fn optics(app: &App) -> Result<Value, String> {
    let o = app.db.exec.run("opticaltst", &["-getpara"], T).await?;
    Ok(optics_from(&o.stdout))
}

/// Switch ports 0-3 are the four LAN sockets.
async fn lan_ports(app: &App) -> Result<Value, String> {
    let mut rows = Vec::new();
    for p in 0..4u8 {
        let ps = p.to_string();
        let st = app.db.exec.run("siswitch", &["-gplinkstatus", &ps], T).await?;
        let m = kv_lines(&st.stdout);
        let link = num(&m, "link") == Some(1.0);
        let speed = match num(&m, "speed") {
            Some(s) if s == 2.0 => 1000,
            Some(s) if s == 1.0 => 100,
            _ => 10,
        };
        let stats = app.db.exec.run("siswitch", &["-gpstatics", &ps], T).await?;
        let s = kv_lines(&stats.stdout);
        rows.push(json!({
            "port": p,
            "label": format!("LAN{}", p + 1),
            "link": link,
            "speedMbps": if link { Some(speed) } else { None },
            "fullDuplex": link && num(&m, "duplex") == Some(1.0),
            "rxBytes": num(&s, "Statistics1.inbytes"),
            "txBytes": num(&s, "Statistics1.outbytes"),
            "rxErrors": num(&s, "Statistics1.inerror"),
            "txErrors": num(&s, "Statistics1.outerror"),
        }));
    }
    Ok(json!({ "rows": rows }))
}

/// `sidbg 1 adev_mgr list_adev`: blank-line separated `Key: value` blocks.
pub fn parse_adev(s: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut cur = Map::new();
    for l in s.lines().chain(std::iter::once("")) {
        let l = l.trim();
        if l.is_empty() {
            if cur.contains_key("MAC") {
                out.push(Value::Object(std::mem::take(&mut cur)));
            }
            cur.clear();
            continue;
        }
        if let Some((k, v)) = l.split_once(':') {
            cur.insert(k.trim().to_string(), json!(v.trim()));
        }
    }
    out
}

async fn hosts(app: &App) -> Result<Value, String> {
    let o = app.db.exec.run("sidbg", &["1", "adev_mgr", "list_adev"], T).await?;
    let arp = crate::status::arp();
    let rows: Vec<Value> = parse_adev(&o.stdout)
        .into_iter()
        .map(|mut h| {
            let mac = h["MAC"].as_str().unwrap_or("").to_lowercase();
            if let Some(a) = arp.iter().find(|a| a.mac == mac) {
                h["arpIp"] = json!(a.ip);
            }
            h["MAC"] = json!(mac);
            h
        })
        .collect();
    Ok(json!({ "rows": rows }))
}

async fn device_info(app: &App) -> Result<Value, String> {
    let rd = |f: &str| std::fs::read_to_string(format!("/proc/zxic/{f}")).unwrap_or_default().trim().to_string();
    let mut v = json!({
        "model": rd("boardtype"),
        "softwareVersion": rd("softVersion"),
        "hardwareVersion": rd("hardVersion"),
        "bootVersion": rd("bootVersion"),
        "innerVersion": rd("innerVersion"),
        "serialNumber": rd("serialnumber"),
    });
    if let Ok(t) = app.db.read("DevInfo").await {
        if let Some(r) = t.row(0) {
            for (k, val) in &r.fields {
                if ["ManuFacturer", "ModelName", "Description", "ProductClass"].contains(&k.as_str()) {
                    v[k] = json!(val);
                }
            }
        }
    }
    Ok(v)
}

fn read_sys(p: &str) -> String {
    std::fs::read_to_string(p).unwrap_or_default().trim().to_string()
}

/// The front panel, as LED states: ok | warn | err | off | blink.
pub async fn leds(app: &App) -> Value {
    let optics = read(app, "pon_optics").await.unwrap_or(json!({}));
    let ports = read(app, "lan_ports").await.unwrap_or(json!({}));
    let route = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
    let has_default = route.lines().skip(1).any(|l| {
        let c: Vec<&str> = l.split_whitespace().collect();
        c.len() > 2 && c[1] == "00000000" && c[0] != "br0"
    });
    let los = optics["los"].as_bool().unwrap_or(true);
    let up = |i: &str| read_sys(&format!("/sys/class/net/{i}/operstate")) == "up";
    let usb = std::fs::read_dir("/sys/bus/usb/devices")
        .map(|d| d.filter_map(Result::ok).any(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.contains('-') && !n.contains(':')
        }))
        .unwrap_or(false);
    let mut lan = Vec::new();
    if let Some(rows) = ports["rows"].as_array() {
        for r in rows {
            lan.push(json!({ "id": r["label"], "state": if r["link"].as_bool() == Some(true) { "ok" } else { "off" },
                "detail": r["speedMbps"].as_u64().map(|s| format!("{s} Mbit/s")).unwrap_or_else(|| "no link".into()) }));
        }
    }
    let mut v = vec![
        json!({ "id": "PON", "state": if los { "off" } else { "ok" }, "detail": if los { "no optical signal" } else { "optical signal present" } }),
        json!({ "id": "LOS", "state": if los { "err" } else { "off" }, "detail": if los { "fiber not connected or no light" } else { "light OK" } }),
        json!({ "id": "Internet", "state": if has_default { "ok" } else { "off" }, "detail": if has_default { "default route present" } else { "no WAN route" } }),
    ];
    v.extend(lan);
    v.push(json!({ "id": "2.4G", "state": if up("wlan0") { "ok" } else { "off" }, "detail": "" }));
    v.push(json!({ "id": "5G", "state": if up("wlan4") { "ok" } else { "off" }, "detail": "" }));
    v.push(json!({ "id": "USB", "state": if usb { "ok" } else { "off" }, "detail": if usb { "device connected" } else { "empty" } }));
    json!(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optics() {
        let raw = "\n optical temp=11325\n\n optical SupplyVoltage=33228\n optical TXBiasCurrent=7675\n optical TXPower=17581\n optical RXPower=0\n";
        let v = optics_from(raw);
        assert_eq!(v["temperatureC"], json!(44.2));
        assert_eq!(v["supplyVoltageV"], json!(3.32));
        assert_eq!(v["biasCurrentMa"], json!(15.4));
        assert_eq!(v["txPowerDbm"], json!(2.45));
        assert_eq!(v["los"], json!(true));
    }

    #[test]
    fn adev() {
        let s = "        ViewName:        IGD.LD1.Hosts.Adev1\n        MAC:             00:11:22:33:44:55\n        HostName:        phone\n        LastConn:        1970-01-01 08:17:45\n\n        ViewName: x\n";
        let v = parse_adev(s);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["HostName"], json!("phone"));
        assert_eq!(v[0]["LastConn"], json!("1970-01-01 08:17:45"));
    }
}
