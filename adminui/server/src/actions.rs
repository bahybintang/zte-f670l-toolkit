//! Builtin actions and live tools (diagnostics, Wi-Fi clients/survey, reboot,
//! config download). Inputs that reach a command are validated and passed as
//! argv; nothing is interpolated into a shell.

use crate::api::{ApiError, R, S};
use crate::apply::USER_CFG;
use axum::body::Body;
use axum::extract::{Query, State};
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
        .route("/wifi/clients", get(wifi_clients))
        .route("/wifi/survey", get(wifi_survey))
        .route("/clients", get(clients))
        .route("/backup/download", get(backup_download))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert!(valid_host("192.0.2.1"));
        assert!(valid_host("example.com"));
        assert!(!valid_host("-c1"));
        assert!(!valid_host("a;b"));
    }
}
