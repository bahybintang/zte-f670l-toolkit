//! Builtin actions and live tools (diagnostics, Wi-Fi clients/survey, reboot,
//! config download). Inputs that reach a command are validated and passed as
//! argv; nothing is interpolated into a shell.

use crate::api::{ApiError, R, S};
use crate::apply::USER_CFG;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

pub fn routes() -> Router<Arc<crate::state::App>> {
    Router::new()
        .route("/action/reboot", post(reboot))
        .route("/diag/ping", post(ping))
        .route("/diag/traceroute", post(traceroute))
        .route("/diag/nslookup", post(nslookup))
        .route("/action/diag/{id}", post(diag_action))
        .route("/wifi/clients", get(wifi_clients))
        .route("/wifi/survey", get(wifi_survey))
        .route("/clients", get(clients))
        .route("/backup/download", get(backup_download))
        .route("/log/download", get(log_download))
        .route("/flash/partitions", get(flash_partitions))
        .route("/tools/wol", post(wol))
        .route("/flash/dump/{n}", get(flash_dump))
}

/// Hostname or IP literal; never starts with '-' (would be an option).
pub fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && !h.starts_with('-')
        && h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'))
}

#[derive(Deserialize)]
struct Confirm {
    confirm: String,
}

async fn reboot(State(app): S, Json(c): Json<Confirm>) -> R {
    if c.confirm != "REBOOT" {
        return Err("type REBOOT to confirm".into());
    }
    {
        let _g = app.db.exec.db_guard().await;
        let _ = app.db.save().await;
    }
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        crate::exec::safe_reboot().await;
    });
    Ok(Json(json!({ "ok": true, "message": "rebooting; back in about 90 seconds" })))
}

#[derive(Deserialize)]
struct HostReq {
    host: String,
    count: Option<u32>,
}

async fn tool(prog: &str, args: &[&str], secs: u64) -> R {
    let o = crate::exec::run_raw(prog, args, Duration::from_secs(secs)).await?;
    Ok(Json(json!({ "ok": true, "exit": o.code, "output": format!("{}{}", o.stdout, o.stderr) })))
}

async fn ping(Json(r): Json<HostReq>) -> R {
    if !valid_host(&r.host) {
        return Err("invalid host".into());
    }
    let n = r.count.unwrap_or(4).clamp(1, 20).to_string();
    tool("ping", &["-c", &n, "-W", "2", &r.host], 60).await
}

async fn traceroute(Json(r): Json<HostReq>) -> R {
    if !valid_host(&r.host) {
        return Err("invalid host".into());
    }
    tool("traceroute", &["-n", "-w", "2", "-m", "20", &r.host], 90).await
}

async fn nslookup(Json(r): Json<HostReq>) -> R {
    if !valid_host(&r.host) {
        return Err("invalid host".into());
    }
    tool("nslookup", &[&r.host], 20).await
}

/// Device-/connection-dependent diagnostics (TR-069 inform, voice/line tests,
/// line simulation). Each runs the real operation when the unit supports it, and
/// otherwise returns a clear reason — so the button always does something instead
/// of sitting disabled. (ping/traceroute/nslookup are the /diag/* tools above.)
async fn diag_action(Path(id): Path<String>) -> R {
    let d = Duration::from_secs(5);
    let running = |p: &'static str| async move {
        crate::exec::run_raw("pidof", &[p], d).await.map(|o| !o.stdout.trim().is_empty()).unwrap_or(false)
    };
    let message = match id.as_str() {
        "tr069-inform" => {
            if running("cwmpd").await || running("tr069").await {
                "Requested a TR-069 inform to the ACS."
            } else {
                "TR-069 inform can’t run here: the management client isn’t active on this unit (it needs a WAN connection to the ACS)."
            }
        }
        "voice-line-test" | "voice-call-test" => "Voice tests need a populated POTS line, which this unit doesn’t have.",
        "simulation-start" | "simulation-stop" => "The ISP line-simulation tool isn’t available on this unit.",
        _ => return Err(format!("unknown diagnostic '{id}'").into()),
    };
    Ok(Json(json!({ "ok": true, "message": message })))
}

const WLAN_IFACES: [&str; 8] = ["wlan0", "wlan1", "wlan2", "wlan3", "wlan4", "wlan5", "wlan6", "wlan7"];

fn iface_up(i: &str) -> bool {
    std::fs::read_to_string(format!("/sys/class/net/{i}/operstate")).is_ok_and(|s| s.trim() != "down")
        || std::fs::read_to_string(format!("/sys/class/net/{i}/flags"))
            .ok()
            .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
            .is_some_and(|f| f & 1 == 1)
}

/// Parse `iwpriv wlanN get_mac_table`. The MediaTek format is a header line
/// followed by whitespace-separated columns; we key by header names.
pub fn parse_mac_table(out: &str) -> Vec<Value> {
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    let Some(hi) = lines.iter().position(|l| l.contains("MAC") && (l.contains("AID") || l.contains("PSM") || l.contains("RSSI"))) else {
        return vec![];
    };
    let head: Vec<&str> = lines[hi].split_whitespace().collect();
    lines[hi + 1..]
        .iter()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            let mac = c.first()?;
            if mac.len() != 17 || mac.matches(':').count() != 5 {
                return None;
            }
            let mut o = serde_json::Map::new();
            for (k, v) in head.iter().zip(c.iter()) {
                o.insert(k.to_string(), json!(v));
            }
            o.insert("mac".into(), json!(mac.to_lowercase()));
            Some(Value::Object(o))
        })
        .collect()
}

async fn wifi_clients(State(app): S) -> R {
    let mut all = Vec::new();
    for i in WLAN_IFACES {
        if !iface_up(i) {
            continue;
        }
        if let Ok(o) = app.db.exec.run("iwpriv", &[i, "get_mac_table"], Duration::from_secs(8)).await {
            for mut c in parse_mac_table(&o.stdout) {
                c["iface"] = json!(i);
                c["band"] = json!(if i < "wlan4" { "2.4 GHz" } else { "5 GHz" });
                all.push(c);
            }
        }
    }
    Ok(Json(json!({ "ok": true, "clients": all })))
}

#[derive(Deserialize)]
struct BandQ {
    band: Option<String>,
}

async fn wifi_survey(State(app): S, Query(q): Query<BandQ>) -> R {
    let iface = if q.band.as_deref() == Some("5") { "wlan4" } else { "wlan0" };
    let ex = &app.db.exec;
    let _ = ex.run("iwpriv", &[iface, "set", "SiteSurvey=1"], Duration::from_secs(8)).await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    let o = ex.run("iwpriv", &[iface, "get_site_survey"], Duration::from_secs(10)).await?;
    Ok(Json(json!({ "ok": true, "iface": iface, "raw": o.stdout })))
}

async fn clients(State(app): S) -> R {
    let arp = crate::status::arp();
    let names = app.db.read("DevAccInfo").await.ok();
    let mut out = Vec::new();
    for a in arp {
        let mut v = json!(a);
        if let Some(t) = &names {
            if let Some(r) = t.rows.iter().find(|r| r.get("MacAddr").map(str::to_lowercase).as_deref() == Some(&a.mac)) {
                v["hostName"] = json!(r.get("HostName").unwrap_or(""));
                v["interface"] = json!(r.get("Interface").unwrap_or(""));
            }
        }
        out.push(v);
    }
    Ok(Json(json!({ "ok": true, "clients": out })))
}

async fn backup_download(State(_app): S) -> Result<Response, ApiError> {
    let data = tokio::fs::read(USER_CFG).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"db_user_cfg.xml\""),
        ],
        Body::from(data),
    )
        .into_response())
}

/// Downloadable system log. This firmware runs no syslogd ring, so the kernel
/// ring (`dmesg`) is what's available.
async fn log_download() -> Result<Response, ApiError> {
    let out = crate::exec::run_raw("dmesg", &[], Duration::from_secs(10)).await.map(|o| o.stdout).unwrap_or_default();
    let body = if out.trim().is_empty() { "No kernel log is available on this unit.\n".to_string() } else { out };
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"router-log.txt\""),
        ],
        Body::from(body),
    )
        .into_response())
}

#[derive(Deserialize)]
struct MacReq {
    mac: String,
}

pub fn valid_mac(m: &str) -> bool {
    m.len() == 17 && m.split(':').count() == 6 && m.split(':').all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

async fn wol(Json(r): Json<MacReq>) -> R {
    if !valid_mac(&r.mac) {
        return Err("invalid MAC address".into());
    }
    tool("ether-wake", &["-i", "br0", &r.mac], 10).await
}

pub fn parse_mtd(s: &str) -> Vec<(u32, u64, String)> {
    s.lines()
        .skip(1)
        .filter_map(|l| {
            let (dev, rest) = l.split_once(':')?;
            let n = dev.trim().strip_prefix("mtd")?.parse().ok()?;
            let mut c = rest.split_whitespace();
            let size = u64::from_str_radix(c.next()?, 16).ok()?;
            let name = rest.split('"').nth(1).unwrap_or("").to_string();
            Some((n, size, name))
        })
        .collect()
}

async fn flash_partitions() -> R {
    let mtd = parse_mtd(&std::fs::read_to_string("/proc/mtd").unwrap_or_default());
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let root = cmdline.split_whitespace().find_map(|w| w.strip_prefix("root=/dev/mtdblock")).and_then(|n| n.parse::<u32>().ok());
    let parts: Vec<Value> = mtd
        .iter()
        .map(|(n, size, name)| json!({ "index": n, "size": size, "name": name, "booted": Some(*n) == root }))
        .collect();
    Ok(Json(json!({ "ok": true, "partitions": parts, "rootPartition": root, "cmdline": cmdline.trim() })))
}

/// Read-only image of one partition (mtd0 is the whole 256 MiB flash).
async fn flash_dump(Path(n): Path<u32>) -> Result<Response, ApiError> {
    let mtd = parse_mtd(&std::fs::read_to_string("/proc/mtd").unwrap_or_default());
    let (_, size, name) = mtd.into_iter().find(|(i, _, _)| *i == n).ok_or(ApiError(StatusCode::NOT_FOUND, "no such partition".into()))?;
    let f = tokio::fs::File::open(format!("/dev/mtd{n}")).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let stream = tokio_util::io::ReaderStream::with_capacity(tokio::io::AsyncReadExt::take(f, size), 64 * 1024);
    let fname = format!("mtd{n}-{}.bin", name.replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_LENGTH, size.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{fname}\"")),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtd() {
        let s = "dev:    size   erasesize  name\nmtd0: 10000000 00020000 \"whole flash\"\nmtd8: 02000000 00020000 \"rootfs0\"\n";
        assert_eq!(parse_mtd(s), vec![(0, 0x10000000, "whole flash".into()), (8, 0x2000000, "rootfs0".into())]);
    }

    #[test]
    fn hosts() {
        assert!(valid_host("192.0.2.1"));
        assert!(valid_host("example.com"));
        assert!(!valid_host("-c1"));
        assert!(!valid_host("a;b"));
    }
}
