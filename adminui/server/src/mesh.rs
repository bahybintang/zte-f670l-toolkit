//! EasyMesh on the MediaTek stack the firmware ships but never starts:
//! `wapp`, `p1905_managerd` (`-r0` controller, `-r1` agent) and `mapd`.
//!
//! The daemons read fixed paths in `/wlan`, which is persistent flash holding
//! the radio calibration files, and the copies there are SDK templates. The
//! real config is generated into tmpfs and bind-mounted over the files the
//! daemons read, so flash is never written and a reboot reverts everything.
//! See notes/easymesh.md.

use serde::{Deserialize, Serialize};
use std::path::Path;

const SETTINGS_FILE: &str = "mesh.json";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Controller,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backhaul {
    Ethernet,
    Wifi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub enabled: bool,
    pub role: Role,
    pub backhaul: Backhaul,
    /// Network every mesh node broadcasts.
    pub ssid: String,
    /// WPA2 passphrase of that network. Never returned by the API.
    pub password: String,
    pub band_steering: bool,
    /// Client signal (dBm) below which a better node is looked for, per band.
    pub roam_rssi_24: i32,
    pub roam_rssi_5: i32,
    /// Signal (dBm) at which a node steers a client to another node.
    pub steer_rssi: i32,
    /// Passphrase of the hidden node-to-node network; generated once.
    pub backhaul_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            role: Role::Controller,
            backhaul: Backhaul::Ethernet,
            ssid: String::new(),
            password: String::new(),
            band_steering: true,
            roam_rssi_24: -75,
            roam_rssi_5: -75,
            steer_rssi: -54,
            backhaul_key: String::new(),
        }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(SETTINGS_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        std::fs::rename(&tmp, dir.join(SETTINGS_FILE)).map_err(|e| e.to_string())
    }

    /// The MediaTek BSS list is space-separated, so names and keys take no spaces.
    pub fn validate(&self) -> Result<(), String> {
        let plain = |s: &str| s.chars().all(|c| c.is_ascii_graphic());
        // 29, not 32: the hidden backhaul network is named "<ssid>-bh".
        if self.ssid.is_empty() || self.ssid.len() > 29 || !plain(&self.ssid) {
            return Err("Mesh Wi-Fi name: 1–29 characters, no spaces".into());
        }
        if self.enabled && self.role == Role::Controller && !(8..=63).contains(&self.password.len()) {
            return Err("Mesh Wi-Fi password: 8–63 characters".into());
        }
        if !plain(&self.password) {
            return Err("Mesh Wi-Fi password: no spaces or accented characters".into());
        }
        let limits = [
            ("2.4 GHz roaming threshold", self.roam_rssi_24),
            ("5 GHz roaming threshold", self.roam_rssi_5),
            ("Steering threshold", self.steer_rssi),
        ];
        for (name, v) in limits {
            if !(-95..=-30).contains(&v) {
                return Err(format!("{name}: between -95 and -30 dBm"));
            }
        }
        Ok(())
    }
}

// --- Config generation -------------------------------------------------------

pub const IFACES_24: [&str; 4] = ["wlan0", "wlan1", "wlan2", "wlan3"];
pub const IFACES_5: [&str; 4] = ["wlan4", "wlan5", "wlan6", "wlan7"];
const BSS_PRIORITY: &str = "wlan0;wlan1;wlan2;wlan3;wlan4;wlan5;wlan6;wlan7;apcli0;apcli1";

/// Interface addresses of this unit, by name.
pub struct Hw {
    pub br: String,
    pub macs: std::collections::HashMap<String, String>,
}

impl Hw {
    fn mac(&self, i: &str) -> &str {
        self.macs.get(i).map(String::as_str).unwrap_or("00:00:00:00:00:00")
    }
}

/// map_cfg.txt: identity, role and backhaul for p1905_managerd and wapp.
pub fn map_cfg(s: &Settings, hw: &Hw) -> String {
    let root = (s.role == Role::Controller) as u8;
    let bh = if s.backhaul == Backhaul::Ethernet { "eth" } else { "wifi" };
    format!(
        "map_controller_alid={br}\nmap_agent=1\nmap_root={root}\nmap_agent_alid={br}\nbh_type={bh}\n\
         radio_band=24G;5G;\nbr_inf=br0\nlan=eth0;eth1;eth2;eth3\nbss_config_priority={BSS_PRIORITY}\n",
        br = hw.br
    )
}

/// wifi_info.txt: the radios' interfaces (the template lists a wlan8 this unit lacks).
pub fn wifi_info(hw: &Hw) -> String {
    let mut out = String::from("inf_count=10\n");
    for i in IFACES_24 {
        out += &format!("interface={i};wiap;2.4g;N;{};00:00:00:00:01:00;\n", hw.mac(i));
    }
    for i in IFACES_5 {
        out += &format!("interface={i};wiap;5g;AC;{};00:00:00:00:02:00;\n", hw.mac(i));
    }
    out += &format!("interface=apcli0;wista;2.4g;N;{};00:00:00:00:01:00;\n", hw.mac("apcli0"));
    out += &format!("interface=apcli1;wista;5g;AC;{};00:00:00:00:02:00;\n", hw.mac("apcli1"));
    out
}

/// mapd_cfg.txt: the SDK template with role, steering and thresholds set.
pub fn mapd_cfg(s: &Settings, template: &str) -> String {
    let set: [(&str, String); 7] = [
        ("MapEnable", "1".into()),
        ("DeviceRole", if s.role == Role::Controller { "1" } else { "2" }.into()),
        ("SteerEnable", (s.band_steering as u8).to_string()),
        ("ScanThreshold2g", s.roam_rssi_24.to_string()),
        ("ScanThreshold5g", s.roam_rssi_5.to_string()),
        ("APSteerRssiTh", s.steer_rssi.to_string()),
        ("bss_config_priority", BSS_PRIORITY.into()),
    ];
    let mut seen = Vec::new();
    let mut out = String::new();
    for line in template.lines() {
        let key = line.split('=').next().unwrap_or("");
        match set.iter().find(|(k, _)| *k == key) {
            Some((k, v)) => {
                out += &format!("{k}={v}\n");
                seen.push(*k);
            }
            None => out += &format!("{line}\n"),
        }
    }
    for (k, v) in &set {
        if !seen.contains(k) {
            out += &format!("{k}={v}\n");
        }
    }
    out
}

/// wts_bss_info_config: the networks the controller gives every node. Columns:
/// index,AL-MAC (ff… = all) operating-class SSID auth encryption key
/// backhaul fronthaul hidden. 8x = 2.4 GHz, 11x/12x = 5 GHz low/high;
/// 0x0020/0x0008 = WPA2-Personal/AES.
pub fn bss_list(s: &Settings) -> String {
    let mut out = String::from("#ucc_bss_info\n");
    let mut n = 0;
    for (ssid, key, bh, fh, hidden) in [(&s.ssid, &s.password, 0, 1, "N"), (&format!("{}-bh", s.ssid), &s.backhaul_key, 1, 0, "Y")] {
        for class in ["8x", "11x", "12x"] {
            n += 1;
            out += &format!("{n},ff:ff:ff:ff:ff:ff {class} {ssid} 0x0020 0x0008 {key} {bh} {fh} hidden-{hidden}\n");
        }
    }
    out
}

/// The 5 GHz Hotspot 2.0 config for wapp: the wlan0 one with its interfaces moved.
pub fn wapp_5g(template_24: &str) -> String {
    template_24
        .lines()
        .map(|l| match l {
            "interface=wlan0" => "interface=wlan4",
            "osu_interface=wlan1" => "osu_interface=wlan5",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// wapp_ap.conf: which per-radio wapp configs to load.
pub fn wapp_list(run_dir: &str) -> String {
    format!("conf_list=/wlan/wapp_ap_wlan0.conf;{run_dir}/wapp_ap_wlan4.conf\n")
}

// --- Running the stack -------------------------------------------------------

pub const RUN_DIR: &str = "/var/mesh";
/// LD_PRELOAD shim for wapp: realigns the driver's WAPP events (mesh/wappfix.c).
const WAPPFIX_SO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wappfix.so"));
/// What wapp runs to apply settings the controller sends (mesh/wifi_config_save).
const WIFI_CONFIG_SAVE: &str = include_str!("../../mesh/wifi_config_save");
const WLAN: &str = "/wlan";
/// Files the daemons read from /wlan that the generated ones are bound over.
const BOUND: [&str; 5] = ["map_cfg.txt", "wifi_info.txt", "mapd_cfg.txt", "wts_bss_info_config", "wapp_ap.conf"];
/// Start order; stop goes in reverse.
const DAEMONS: [&str; 3] = ["wapp", "p1905_managerd", "mapd"];
/// Radio bring-up after boot takes about a minute; start the mesh after it.
pub const BOOT_DELAY: std::time::Duration = std::time::Duration::from_secs(90);
const T: std::time::Duration = std::time::Duration::from_secs(10);

async fn sh(cmd: &str) -> Result<crate::exec::Output, String> {
    crate::exec::run_raw("sh", &["-c", cmd], T).await
}

fn read_hw() -> Hw {
    let addr = |i: &str| std::fs::read_to_string(format!("/sys/class/net/{i}/address")).ok().map(|s| s.trim().to_string());
    let mut macs = std::collections::HashMap::new();
    for i in IFACES_24.iter().chain(IFACES_5.iter()).chain(["apcli0", "apcli1"].iter()) {
        if let Some(m) = addr(i) {
            macs.insert(i.to_string(), m);
        }
    }
    Hw { br: addr("br0").unwrap_or_default(), macs }
}

pub async fn running() -> Vec<(&'static str, bool)> {
    let mut out = Vec::new();
    for d in DAEMONS {
        let up = crate::exec::run_raw("pidof", &[d], T).await.is_ok_and(|o| !o.stdout.trim().is_empty());
        out.push((d, up));
    }
    out
}

/// Stop the daemons and remove the bind mounts (idempotent).
pub async fn stop() {
    for d in DAEMONS.iter().rev() {
        let _ = crate::exec::run_raw("killall", &[d], T).await;
    }
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    for d in DAEMONS {
        let _ = crate::exec::run_raw("killall", &["-9", d], T).await;
    }
    for f in BOUND {
        let _ = crate::exec::run_raw("umount", &[&format!("{WLAN}/{f}")], T).await;
    }
}

/// Generate the config into tmpfs, bind it over /wlan and start the daemons.
pub async fn start(s: &Settings) -> Result<(), String> {
    stop().await; // also unbinds, so the templates below are the /wlan originals
    let rd = |f: &str| std::fs::read_to_string(format!("{WLAN}/{f}")).map_err(|e| format!("{WLAN}/{f}: {e}"));
    let mapd_template = rd("mapd_cfg.txt")?;
    let wapp_template = rd("wapp_ap_wlan0.conf")?;
    let hw = read_hw();
    if hw.br.is_empty() {
        return Err("br0 has no address".into());
    }
    std::fs::create_dir_all(RUN_DIR).map_err(|e| e.to_string())?;
    let files = [
        ("map_cfg.txt", map_cfg(s, &hw)),
        ("wifi_info.txt", wifi_info(&hw)),
        ("mapd_cfg.txt", mapd_cfg(s, &mapd_template)),
        ("wts_bss_info_config", bss_list(s)),
        ("wapp_ap.conf", wapp_list(RUN_DIR)),
        ("wapp_ap_wlan4.conf", wapp_5g(&wapp_template)),
    ];
    for (name, body) in &files {
        write_private(&format!("{RUN_DIR}/{name}"), body)?;
    }
    for f in BOUND {
        let o = crate::exec::run_raw("mount", &["--bind", &format!("{RUN_DIR}/{f}"), &format!("{WLAN}/{f}")], T).await?;
        if !o.ok() {
            stop().await;
            return Err(format!("bind {f}: {}", o.stderr.trim()));
        }
    }
    if WAPPFIX_SO.is_empty() {
        stop().await;
        return Err("this build has no wapp compatibility shim (zig was missing at build time)".into());
    }
    std::fs::create_dir_all(format!("{RUN_DIR}/bin")).map_err(|e| e.to_string())?;
    write_bytes(&format!("{RUN_DIR}/wappfix.so"), WAPPFIX_SO, 0o600)?;
    write_bytes(&format!("{RUN_DIR}/bin/wifi_config_save"), WIFI_CONFIG_SAVE.as_bytes(), 0o700)?;
    let role = if s.role == Role::Controller { "0" } else { "1" };
    // Detached via `sh -c '… &'` so init reaps them; arguments are fixed strings.
    // wapp logs the Wi-Fi passphrases it receives, so its output is discarded;
    // the other two logs carry no key material and back the topology view.
    let cmds = [
        (format!("PATH={RUN_DIR}/bin:$PATH LD_PRELOAD={RUN_DIR}/wappfix.so wapp -d1"), "/dev/null".to_string()),
        (format!("p1905_managerd -r{role}"), format!("{RUN_DIR}/p1905.log")),
        ("mapd".to_string(), format!("{RUN_DIR}/mapd.log")),
    ];
    for (c, log) in &cmds {
        sh(&format!("cd {RUN_DIR} && umask 077 && {c} >{log} 2>&1 </dev/null &")).await?;
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
    let down: Vec<&str> = running().await.into_iter().filter(|(_, up)| !up).map(|(d, _)| d).collect();
    if !down.is_empty() {
        stop().await;
        return Err(format!("{} exited right after start", down.join(", ")));
    }
    Ok(())
}

fn write_bytes(path: &str, body: &[u8], mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).map_err(|e| format!("{path}: {e}"))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| format!("{path}: {e}"))
}

fn write_private(path: &str, body: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).map_err(|e| format!("{path}: {e}"))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| format!("{path}: {e}"))
}

/// Daemon logs are capped: past this size a log is emptied (tmpfs is RAM).
const LOG_MAX: u64 = 256 * 1024;

pub fn trim_logs() {
    for l in ["p1905", "mapd"] {
        let p = format!("{RUN_DIR}/{l}.log");
        if std::fs::metadata(&p).is_ok_and(|m| m.len() > LOG_MAX) {
            let _ = std::fs::OpenOptions::new().write(true).truncate(true).open(&p);
        }
    }
}

/// The 1905 topology. The daemon prints the dump on its own output (our log),
/// and that output is block-buffered (4 KiB), so a dump only reaches the log
/// once more output follows it. Ask for dumps until one is complete, i.e. the
/// next dump's header has arrived after it, and return that one.
pub async fn topology(role: Role) -> String {
    let log = format!("{RUN_DIR}/p1905.log");
    let before = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
    let ctrl = if role == Role::Controller { "controller" } else { "agent" };
    for _ in 0..6 {
        let _ = crate::exec::run_raw("1905ctrl", &[ctrl, "dump_topology_info"], T).await;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let all = std::fs::read(&log).unwrap_or_default();
        let new = String::from_utf8_lossy(all.get(before as usize..).unwrap_or(&[])).into_owned();
        if let Some(dump) = first_complete_dump(&strip_ansi(&new)) {
            return dump;
        }
    }
    String::new()
}

const DUMP_START: &str = "====topology discovery====";

/// The first dump in `log` that is followed by another dump's start, with the
/// `[1905Daemon][dump_topology_info] ` prefixes removed.
pub fn first_complete_dump(log: &str) -> Option<String> {
    let mut starts = log.match_indices(DUMP_START).map(|(i, _)| i);
    let a = starts.next()?;
    let b = starts.next()?;
    let a = log[..a].rfind('\n').map_or(0, |i| i + 1);
    let b = log[..b].rfind('\n').map_or(0, |i| i + 1);
    let lines: Vec<&str> = log[a..b]
        .lines()
        .filter(|l| l.contains("[dump_topology_info]"))
        .map(|l| l.split("[dump_topology_info] ").nth(1).unwrap_or(l).trim_end())
        .collect();
    Some(lines.join("\n"))
}

/// Agent onboarding as the 1905 daemon logs it: the controller it found, and
/// whether every radio finished WSC autoconfiguration (M1/M2) with it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Onboarding {
    pub controller: Option<String>,
    pub configured: bool,
}

pub fn onboarding(log: &str) -> Onboarding {
    let controller = log
        .rsplit("find the conroller al_mac=")
        .next()
        .filter(|_| log.contains("find the conroller al_mac="))
        .map(|t| t.chars().take(17).collect::<String>());
    Onboarding { controller, configured: log.contains("auto configuration done of all radio") }
}

/// One mesh device from a topology dump.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Node {
    pub almac: String,
    /// How it reaches this router, e.g. "LAN4" (from the discovery section).
    pub via: String,
    pub radios: Vec<String>,
    /// Non-1905 devices on its Wi-Fi interfaces, i.e. its clients.
    pub clients: usize,
}

/// IEEE 1905.1 media types seen on this stack.
fn media(code: &str) -> Option<&'static str> {
    Some(match code {
        "0x0100" | "0x0101" => "802.11b/g 2.4 GHz",
        "0x0102" => "802.11a 5 GHz",
        "0x0103" => "802.11n 2.4 GHz",
        "0x0104" => "802.11n 5 GHz",
        "0x0105" => "802.11ac 5 GHz",
        "0x0106" | "0x0107" => "802.11ad 60 GHz",
        "0x0108" => "802.11ax",
        _ => return None,
    })
}

/// The mesh devices in a topology dump (see `first_complete_dump`).
pub fn parse_nodes(dump: &str) -> Vec<Node> {
    let mac = regex_lite::Regex::new(r"([0-9a-f]{2}(?::[0-9a-f]{2}){5})").expect("regex");
    let mut via: Vec<(String, String)> = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();
    let mut section = "";
    let mut pending_neighbor: Option<String> = None;
    let mut want = "";
    let mut on_wifi = false;
    for line in dump.lines().map(str::trim) {
        if line.starts_with("====") {
            section = if line.contains("discovery") { "disc" } else if line.contains("response") { "resp" } else { "" };
            continue;
        }
        let m = mac.captures(line).map(|c| c[1].to_string());
        match section {
            "disc" => {
                if line.ends_with("AL MAC:") {
                    want = "al";
                } else if line.starts_with("receive ") {
                    want = "port";
                } else if let Some(m) = m {
                    if want == "al" {
                        pending_neighbor = Some(m);
                    } else if want == "port" {
                        let port = line.split("switch_port[").nth(1).and_then(|p| p.split(']').next()).and_then(|n| n.parse::<u32>().ok());
                        if let Some(n) = pending_neighbor.take() {
                            via.push((n, port.map_or("Ethernet".into(), |p| format!("LAN{}", p + 1))));
                        }
                    }
                    want = "";
                }
            }
            "resp" => {
                if line.contains("AL MAC(") {
                    if let Some(m) = m {
                        let v = via.iter().find(|(a, _)| *a == m).map(|(_, v)| v.clone()).unwrap_or_else(|| "Wi-Fi".into());
                        nodes.push(Node { almac: m, via: v, radios: Vec::new(), clients: 0 });
                    }
                } else if let Some(code) = line.strip_prefix("media type = ") {
                    on_wifi = false;
                    if let (Some(n), Some(r)) = (nodes.last_mut(), media(code.trim())) {
                        on_wifi = true;
                        if !n.radios.iter().any(|x| x == r) {
                            n.radios.push(r.to_string());
                        }
                    }
                } else if line.starts_with("non p1905 neighbor device") && on_wifi {
                    if let Some(n) = nodes.last_mut() {
                        n.clients += 1;
                    }
                }
            }
            _ => {}
        }
    }
    nodes
}

pub fn strip_ansi(s: &str) -> String {
    let re = regex_lite::Regex::new(r"\x1b\[[0-9;]*m").expect("regex");
    re.replace_all(s, "").trim().to_string()
}

// --- API ----------------------------------------------------------------------

use crate::api::{R, S};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

/// Start/stop never overlap.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn routes() -> Router<std::sync::Arc<crate::state::App>> {
    Router::new().route("/mesh", get(read).post(write)).route("/mesh/status", get(status))
}

/// Settings as the page sees them: the passphrases are reported only as set/unset.
fn public(s: &Settings) -> Value {
    let mut v = serde_json::to_value(s).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("password");
        o.remove("backhaulKey");
        o.insert("passwordSet".into(), json!(!s.password.is_empty()));
    }
    v
}

async fn status_value(role: Role) -> Value {
    trim_logs();
    let procs: serde_json::Map<String, Value> = running().await.into_iter().map(|(d, up)| (d.to_string(), json!(up))).collect();
    let all = procs.values().all(|v| v == true);
    let topo = if all { topology(role).await } else { String::new() };
    let log = std::fs::read(format!("{RUN_DIR}/p1905.log")).map(|b| strip_ansi(&String::from_utf8_lossy(&b))).unwrap_or_default();
    let agent = (all && role == Role::Agent).then(|| onboarding(&log));
    json!({ "running": all, "daemons": procs, "nodes": parse_nodes(&topo), "topology": topo, "agent": agent })
}

async fn read(State(app): S) -> R {
    let mut s = Settings::load(&app.dir);
    if s.ssid.is_empty() {
        // Default the mesh network to the main 2.4 GHz SSID.
        if let Ok(t) = app.db.read("WLANCfg").await {
            s.ssid = t.row(0).and_then(|r| r.get("ESSID")).unwrap_or("").replace(' ', "-");
        }
    }
    Ok(Json(json!({ "ok": true, "settings": public(&s), "status": status_value(s.role).await })))
}

async fn status(State(app): S) -> R {
    let role = Settings::load(&app.dir).role;
    Ok(Json(json!({ "ok": true, "status": status_value(role).await })))
}

fn new_key() -> Result<String, String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|e| e.to_string())?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

async fn write(State(app): S, Json(mut s): Json<Settings>) -> R {
    let _g = LOCK.lock().await;
    let old = Settings::load(&app.dir);
    // Write-only secrets: an empty value keeps the stored one.
    if s.password.is_empty() {
        s.password = old.password.clone();
    }
    s.backhaul_key = if old.backhaul_key.is_empty() { new_key()? } else { old.backhaul_key.clone() };
    s.validate()?;
    s.save(&app.dir)?;
    let message = if s.enabled {
        start(&s).await?;
        if s.role == Role::Controller {
            "EasyMesh is running. Other EasyMesh devices can join now."
        } else {
            "EasyMesh agent started. It joins the controller on the cabled LAN port within a minute."
        }
    } else {
        stop().await;
        "EasyMesh is off."
    };
    Ok(Json(json!({ "ok": true, "message": message, "settings": public(&s), "status": status_value(s.role).await })))
}

/// At boot: start the mesh if it was left on.
pub async fn resume_on_boot(dir: std::path::PathBuf) {
    let s = Settings::load(&dir);
    if !s.enabled {
        return;
    }
    tokio::time::sleep(BOOT_DELAY).await;
    let _g = LOCK.lock().await;
    if running().await.iter().all(|(_, up)| *up) {
        return; // already up (adminui restarted while the mesh ran)
    }
    if let Err(e) = start(&s).await {
        eprintln!("mesh: start at boot failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw() -> Hw {
        let mut macs = std::collections::HashMap::new();
        macs.insert("wlan0".to_string(), "02:00:00:00:00:01".to_string());
        macs.insert("wlan4".to_string(), "02:00:00:00:00:05".to_string());
        Hw { br: "02:00:00:00:00:aa".into(), macs }
    }

    fn settings() -> Settings {
        Settings { ssid: "Home".into(), password: "example-pass".into(), backhaul_key: "bhkey123".into(), ..Default::default() }
    }

    #[test]
    fn map_cfg_by_role() {
        let c = map_cfg(&settings(), &hw());
        assert!(c.contains("map_controller_alid=02:00:00:00:00:aa\n"));
        assert!(c.contains("map_root=1\n") && c.contains("bh_type=eth\n"));
        assert!(!c.contains("wlan8"));
        let a = map_cfg(&Settings { role: Role::Agent, backhaul: Backhaul::Wifi, ..settings() }, &hw());
        assert!(a.contains("map_root=0\n") && a.contains("bh_type=wifi\n"));
    }

    #[test]
    fn wifi_info_lists_this_units_radios() {
        let w = wifi_info(&hw());
        assert!(w.starts_with("inf_count=10\n"));
        assert!(w.contains("interface=wlan0;wiap;2.4g;N;02:00:00:00:00:01;00:00:00:00:01:00;"));
        assert!(w.contains("interface=wlan4;wiap;5g;AC;02:00:00:00:00:05;00:00:00:00:02:00;"));
        assert!(w.contains("interface=apcli1;wista;5g;"));
        assert_eq!(w.lines().count(), 11);
    }

    #[test]
    fn mapd_cfg_overrides_template_keys() {
        let t = "APSteerRssiTh=-54\nDeviceRole=2\nMapEnable=1\nSteerEnable=1\nlan_interface=eth0\n";
        let s = Settings { band_steering: false, steer_rssi: -60, roam_rssi_5: -70, ..settings() };
        let m = mapd_cfg(&s, t);
        assert!(m.contains("DeviceRole=1\n") && m.contains("SteerEnable=0\n") && m.contains("APSteerRssiTh=-60\n"));
        assert!(m.contains("lan_interface=eth0\n"), "untouched keys kept");
        assert!(m.contains("ScanThreshold5g=-70\n"), "missing keys appended");
        assert_eq!(m.matches("DeviceRole=").count(), 1);
    }

    #[test]
    fn bss_list_fronthaul_and_hidden_backhaul() {
        let b = bss_list(&settings());
        assert_eq!(b.lines().count(), 7);
        assert!(b.contains("1,ff:ff:ff:ff:ff:ff 8x Home 0x0020 0x0008 example-pass 0 1 hidden-N\n"));
        assert!(b.contains("6,ff:ff:ff:ff:ff:ff 12x Home-bh 0x0020 0x0008 bhkey123 1 0 hidden-Y\n"));
    }

    #[test]
    fn wapp_5g_moves_interfaces() {
        let w = wapp_5g("# x\ninterface=wlan0\nosu_interface=wlan1\nicon_path=/wlan/\n");
        assert_eq!(w, "# x\ninterface=wlan4\nosu_interface=wlan5\nicon_path=/wlan/\n");
        assert_eq!(wapp_list("/var/mesh"), "conf_list=/wlan/wapp_ap_wlan0.conf;/var/mesh/wapp_ap_wlan4.conf\n");
    }

    #[test]
    fn complete_dump_only() {
        let one = "[1905Daemon][dump_topology_info] ====================topology discovery====================\n\
                   [1905Daemon][x] noise\n[1905Daemon][dump_topology_info] no 1905.1 neighbors!!!\n";
        assert_eq!(first_complete_dump(one), None, "not followed by another dump yet");
        let two = format!("junk\n{one}{one}");
        let d = first_complete_dump(&two).unwrap();
        assert_eq!(d, "====================topology discovery====================\nno 1905.1 neighbors!!!");
    }

    #[test]
    fn nodes_from_dump() {
        let d = "====================topology discovery====================\nneighbor1 AL MAC:\n02:00:00:00:0c:01\n\
                 neighbor1 connect interface MAC:\n02:00:00:00:0c:01\nreceive neighbor1 interface MAC:\n\
                 02:00:00:00:0a:28 switch_port[3]\n====================topology response====================\n\
                 device1 AL MAC(02:00:00:00:0c:01)\ndevice1 interface1(02:00:00:00:0c:01)\nmedia type = 0x0001\n\
                 p1905 neighbor device1(02:00:00:00:0a:28)\ndevice1 interface2(02:00:00:00:0c:02)\nmedia type = 0x0103\n\
                 non p1905 neighbor device1(02:00:00:00:0d:01)\ndevice1 interface4(02:00:00:00:0c:03)\nmedia type = 0x0105\n\
                 non p1905 neighbor device1(02:00:00:00:0d:02)\nnon p1905 neighbor device2(02:00:00:00:0d:03)\n\
                 ====================local info====================\nlocal MAC 1(02:00:00:00:0a:28) trx_config=0f\n";
        let n = parse_nodes(d);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].almac, "02:00:00:00:0c:01");
        assert_eq!(n[0].via, "LAN4");
        assert_eq!(n[0].radios, vec!["802.11n 2.4 GHz", "802.11ac 5 GHz"]);
        assert_eq!(n[0].clients, 3);
        assert!(parse_nodes("").is_empty());
    }

    #[test]
    fn agent_onboarding_from_log() {
        let log = "[1905Daemon][parse_cmdu_message] find the conroller al_mac=02:00:00:00:0c:01\n\
                   [1905Daemon][ap_autoconfig_enrolle_sm] send WSC M1 to 02:00:00:00:0c:01\n\
                   [1905Daemon][triger_autoconfiguration] auto configuration done of all radio\n";
        assert_eq!(onboarding(log), Onboarding { controller: Some("02:00:00:00:0c:01".into()), configured: true });
        assert_eq!(onboarding("nothing"), Onboarding { controller: None, configured: false });
    }

    #[test]
    fn ansi_is_stripped() {
        assert_eq!(strip_ansi("\x1b[1m\x1b[40;31m(02:00)\x1b[0m tree\n"), "(02:00) tree");
    }

    #[test]
    fn validation() {
        assert!(settings().validate().is_ok());
        assert!(Settings { ssid: "My Home".into(), ..settings() }.validate().is_err());
        assert!(Settings { ssid: "x".repeat(30), ..settings() }.validate().is_err());
        assert!(Settings { enabled: true, password: "short".into(), ..settings() }.validate().is_err());
        assert!(Settings { enabled: true, role: Role::Agent, password: String::new(), ..settings() }.validate().is_ok());
        assert!(Settings { steer_rssi: -20, ..settings() }.validate().is_err());
    }
}
