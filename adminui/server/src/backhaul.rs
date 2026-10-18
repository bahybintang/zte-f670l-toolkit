//! The Wi-Fi backhaul of an EasyMesh agent: the station (apcli) interfaces,
//! joining the main router, and bridging the station into br0 safely.
//!
//! Three things on this firmware decide how that has to be done (all verified
//! 2026-10-17, see notes/easymesh.md):
//! - ZTE's bridge keeps its own port table (/proc/net/brport), filled by the
//!   stock firmware for eth0-3 and wlan0-7 only. A station added to br0
//!   without an entry forwards nothing (permissions 0), and removing it again
//!   hangs the kernel until the hardware watchdog resets the router. Stations
//!   are registered (`brctl setbrport`/`setpermits`) before they are bridged,
//!   and an unregistered one is never removed.
//! - The kernel's loop detection (/proc/net/loopback/test, not switched by the
//!   LoopbackGlobalConf setting) shuts a LAN port for 300 s when the station
//!   connects while a cable also leads to the main router.
//! - br0 runs no STP, so the station is never up alongside such a cable.
//! - ZTE's Wi-Fi "hardware forwarding switch" (/proc/wifi_hdswitch/on, the
//!   2.4 GHz driver's `g_hdfwswitch`) hands every received Wi-Fi frame to the
//!   kernel as if it came from the radio's first AP, so the station's traffic
//!   enters br0 as wlan0 even when the station is not bridged: the bridge
//!   learns the main router's side behind wlan0 and sends replies out of the
//!   2.4 GHz AP. It is switched off while the backhaul runs, so frames arrive
//!   on the station itself (verified 2026-10-18: FDB entries move to apcli1
//!   and traffic flows both ways).

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;

const T: std::time::Duration = std::time::Duration::from_secs(10);

/// The station (apcli) interfaces used for a Wi-Fi backhaul: 5 GHz first.
pub const STATIONS: [&str; 2] = ["apcli1", "apcli0"];
/// Their entries in ZTE's bridge-port table (the stock ports use 0-11).
const STATION_PORTS: [(&str, &str); 2] = [("apcli0", "12"), ("apcli1", "13")];
/// Forwarding permissions of a bridge port: to every other port, as the stock ports have.
const ALL_PORTS: &str = "ffffffff";
/// How long a pairing attempt keeps triggering WPS.
const PAIR_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);
/// WPS walk time (Wi-Fi Simple Configuration: 120 s), plus a little slack.
const WPS_WALK: std::time::Duration = std::time::Duration::from_secs(125);
/// A station left unassociated this long (the mesh daemons switch it off when
/// they see a wired path) is pointed at the main router again.
const RECONNECT_AFTER: std::time::Duration = std::time::Duration::from_secs(30);
/// After a LAN cable goes in, how long to wait for the bridge to learn who is
/// behind it before deciding whether it leads to the main router.
const CABLE_SETTLE: std::time::Duration = std::time::Duration::from_secs(30);
const FDB_RECENT: f32 = 120.0;
const BRPORT: &str = "/proc/net/brport";
const LOOPBACK: &str = "/proc/net/loopback/test";
const HDSWITCH: &str = "/proc/wifi_hdswitch/on";

/// Turn ZTE's Wi-Fi hardware forwarding switch on or off (see the module docs).
fn hdswitch(on: bool) {
    let want = if on { "1" } else { "0" };
    if std::fs::read_to_string(HDSWITCH).is_ok_and(|v| v.trim() != want) {
        if let Err(e) = std::fs::write(HDSWITCH, want) {
            eprintln!("backhaul: {HDSWITCH}: {e}");
        }
    }
}

/// The main router's network, for joining it directly.
#[derive(Clone)]
pub struct Uplink {
    pub ssid: String,
    pub password: String,
}

/// The AP a station interface is associated with, if any.
pub async fn station_link(st: &str) -> Option<String> {
    let o = crate::exec::run_raw("iwconfig", &[st], T).await.ok()?;
    let ap = o.stdout.split("Access Point: ").nth(1)?.split_whitespace().next()?.to_lowercase();
    (ap.len() == 17 && ap.matches(':').count() == 5).then_some(ap)
}

/// Wi-Fi backhaul state: the first associated station and its AP.
pub async fn backhaul_link() -> Option<(&'static str, String)> {
    for st in STATIONS {
        if let Some(ap) = station_link(st).await {
            return Some((st, ap));
        }
    }
    None
}

/// Run EasyMesh backhaul WPS until a station associates or the window closes.
/// The controller must be in its "add satellite"/WPS mode. WPS gets one
/// station at a time and its full walk time: re-triggering restarts the WPS
/// state machine, and two push-button sessions at once trip overlap detection.
pub async fn pair_over_wifi() {
    let deadline = std::time::Instant::now() + PAIR_WINDOW;
    'window: while std::time::Instant::now() < deadline {
        for st in STATIONS {
            if backhaul_link().await.is_some() {
                return;
            }
            for other in STATIONS.iter().filter(|o| **o != st) {
                let _ = crate::exec::run_raw("iwpriv", &[other, "set", "WscStop=1"], T).await;
            }
            let Ok(mac) = std::fs::read_to_string(format!("/sys/class/net/{st}/address")) else { continue };
            let _ = crate::exec::run_raw("mapd_cli", &["/tmp/mapd_ctrl", "trigger_map_wps", mac.trim()], T).await;
            let walk = std::time::Instant::now() + WPS_WALK;
            while std::time::Instant::now() < walk {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                if station_link(st).await.is_some() {
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    break 'window;
                }
            }
        }
    }
}

/// Join the main router's 5 GHz network directly. Arguments go to iwpriv
/// without a shell and are never logged.
pub async fn connect_direct(up: &Uplink) {
    let st = STATIONS[0];
    let ssid = format!("ApCliSsid={}", up.ssid);
    let key = format!("ApCliWPAPSK={}", up.password);
    for kv in ["ApCliEnable=0", ssid.as_str(), "ApCliAuthMode=WPA2PSK", "ApCliEncrypType=AES", key.as_str(), "ApCliEnable=1"] {
        let _ = crate::exec::run_raw("iwpriv", &[st, "set", kv], T).await;
    }
}

// --- ZTE bridge-port table ----------------------------------------------------

fn registered(st: &str) -> bool {
    std::fs::read_to_string(BRPORT).is_ok_and(|t| t.contains(&format!("Name:[{st}]")))
}

/// Give the stations entries in ZTE's bridge-port table (interfaces must exist).
pub async fn register_stations() {
    for (st, idx) in STATION_PORTS {
        let _ = crate::exec::run_raw("brctl", &["setbrport", "br0", st, idx, "1"], T).await;
        let _ = crate::exec::run_raw("brctl", &["setpermits", "br0", idx, ALL_PORTS], T).await;
    }
}

pub fn bridged(st: &str) -> bool {
    Path::new(&format!("/sys/class/net/br0/brif/{st}")).exists()
}

async fn bridge_add(st: &str) -> bool {
    if !registered(st) {
        register_stations().await;
    }
    if !registered(st) {
        eprintln!("backhaul: {st} has no bridge-port entry; not bridging it");
        return false;
    }
    crate::exec::run_raw("brctl", &["addif", "br0", st], T).await.is_ok_and(|o| o.ok())
}

/// Take a station out of br0. An unregistered one would hang the kernel, so it
/// is switched off instead (it then forwards nothing).
pub async fn bridge_remove(st: &str) {
    if !bridged(st) {
        return;
    }
    if registered(st) {
        let _ = crate::exec::run_raw("brctl", &["delif", "br0", st], T).await;
    } else {
        eprintln!("backhaul: {st} is bridged without a bridge-port entry; switching it off instead of removing it");
        let _ = crate::exec::run_raw("iwpriv", &[st, "set", "ApCliEnable=0"], T).await;
    }
}

// --- Announcing the router over the link -------------------------------------

/// How often the router re-announces itself while the Wi-Fi link is in use.
const ANNOUNCE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// A gratuitous ARP request for `ip` from `mac` (broadcast, sender = target).
/// Over a station link the driver rewrites the sender address to the
/// station's own (MAC translation), so the main router and its clients learn
/// where this router now is: frames to the old, wired address would never be
/// delivered over Wi-Fi.
pub fn gratuitous_arp(mac: [u8; 6], ip: [u8; 4]) -> [u8; 42] {
    let mut f = [0u8; 42];
    f[..6].copy_from_slice(&[0xff; 6]);
    f[6..12].copy_from_slice(&mac);
    f[12..14].copy_from_slice(&[0x08, 0x06]); // ARP
    f[14..22].copy_from_slice(&[0, 1, 0x08, 0, 6, 4, 0, 1]); // Ethernet/IPv4, request
    f[22..28].copy_from_slice(&mac);
    f[28..32].copy_from_slice(&ip);
    f[38..42].copy_from_slice(&ip); // target hardware address stays zero
    f
}

fn br0_identity() -> Option<([u8; 6], [u8; 4])> {
    let mac = mac_bytes(std::fs::read_to_string("/sys/class/net/br0/address").ok()?.trim())?;
    match crate::lan_ip() {
        std::net::IpAddr::V4(ip) => Some((mac, ip.octets())),
        std::net::IpAddr::V6(_) => None,
    }
}

#[cfg(target_os = "linux")]
fn send_frame(ifname: &str, frame: &[u8]) -> bool {
    let Ok(name) = std::ffi::CString::new(ifname) else { return false };
    // SAFETY: a raw packet socket owned and closed here; the buffers outlive the calls.
    unsafe {
        let fd = libc::socket(libc::AF_PACKET, libc::SOCK_RAW, 0);
        if fd < 0 {
            return false;
        }
        let mut sll: libc::sockaddr_ll = std::mem::zeroed();
        sll.sll_family = libc::AF_PACKET as u16;
        sll.sll_ifindex = libc::if_nametoindex(name.as_ptr()) as i32;
        sll.sll_halen = 6;
        let sent = libc::sendto(
            fd,
            frame.as_ptr() as *const libc::c_void,
            frame.len(),
            0,
            &sll as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_ll>() as u32,
        );
        libc::close(fd);
        sent == frame.len() as isize
    }
}

#[cfg(not(target_os = "linux"))]
fn send_frame(_ifname: &str, _frame: &[u8]) -> bool {
    false
}

/// Announce br0's address into the bridge a few times.
async fn announce(times: u32) {
    let Some((mac, ip)) = br0_identity() else { return };
    let frame = gratuitous_arp(mac, ip);
    for i in 0..times {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        if !send_frame("br0", &frame) {
            eprintln!("backhaul: gratuitous ARP on br0 failed");
            return;
        }
    }
}

// --- State and supervision ----------------------------------------------------

/// Where the Wi-Fi backhaul stands, for the status view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Off,
    /// Station not associated yet.
    Connecting,
    /// Associated, but a cable to the main router is in (or a LAN port is
    /// shut by loop detection): bridging the station too would close a loop.
    Cabled,
    /// Associated and bridged into br0.
    Bridged,
}

static STATE: AtomicU8 = AtomicU8::new(0);
/// Bumped by every start/stop; a supervisor exits once it is outdated.
static GEN: AtomicU64 = AtomicU64::new(0);
/// The AP the station last associated with: lets an unassociated station tell
/// whether the main router is behind a LAN cable.
static LAST_AP: Mutex<String> = Mutex::new(String::new());

fn set_state(u: State) {
    STATE.store(u as u8, Ordering::SeqCst);
}

pub fn state() -> State {
    match STATE.load(Ordering::SeqCst) {
        1 => State::Connecting,
        2 => State::Cabled,
        3 => State::Bridged,
        _ => State::Off,
    }
}

/// Bring the stations up and start supervising them (agent with a Wi-Fi backhaul).
pub async fn start(uplink: Option<Uplink>) {
    // The firmware leaves its station interfaces down; wapp only registers
    // active ones, and mapd can only run backhaul WPS on registered ones.
    for st in STATIONS {
        let _ = crate::exec::run_raw("ifconfig", &[st, "up"], T).await;
    }
    register_stations().await;
    hdswitch(false);
    // The supervisor joins the main router after RECONNECT_AFTER rather than
    // right away: with a cable to it still in, connecting now would make loop
    // detection shut that LAN port for 300 s, and this leaves time to unplug.
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    set_state(State::Connecting);
    tokio::spawn(supervise(gen, uplink));
}

/// Stop supervising and take the stations out of service (idempotent).
pub async fn stop() {
    GEN.fetch_add(1, Ordering::SeqCst);
    set_state(State::Off);
    for st in STATIONS {
        bridge_remove(st).await;
        let _ = crate::exec::run_raw("iwpriv", &[st, "set", "ApCliEnable=0"], T).await;
    }
    hdswitch(true); // only once the stations are off: it files their frames under wlan0
}

/// LAN sockets with a cable in (switch ports 0-3; the ethN netdevs always
/// report a carrier, so the switch is asked). A port shut by loop detection
/// reads as no cable.
async fn lan_carriers() -> Vec<u8> {
    let mut up = Vec::new();
    for p in 0..4u8 {
        let o = crate::exec::run_raw("siswitch", &["-gplinkstatus", &p.to_string()], T).await;
        if o.is_ok_and(|o| o.stdout.lines().any(|l| l.trim().replace(' ', "") == "link=1")) {
            up.push(p);
        }
    }
    up
}

/// LAN ports the kernel's loop detection has shut, from /proc/net/loopback/test
/// (per port: `Port: eth0`, `Already_loop_flag: 0`, `Port_status: 1`).
pub fn loop_shut_ports(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut port = String::new();
    for l in text.lines() {
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.trim();
        match k.trim() {
            "Port" => port = v.to_string(),
            "Already_loop_flag" if port.starts_with("eth") && !v.is_empty() && v != "0" => out.push(port.clone()),
            "Port_status" if port.starts_with("eth") && v == "0" => out.push(port.clone()),
            _ => {}
        }
    }
    out.dedup();
    out
}

fn loop_shut() -> bool {
    std::fs::read_to_string(LOOPBACK).is_ok_and(|t| !loop_shut_ports(&t).is_empty())
}

fn mac_bytes(m: &str) -> Option<[u8; 6]> {
    let v: Vec<u8> = m.split(':').filter_map(|x| u8::from_str_radix(x, 16).ok()).collect();
    v.try_into().ok()
}

/// Whether two addresses look like the same device (a router's BSSIDs and
/// LAN address are usually a few apart in one block).
pub fn same_device(a: &str, b: &str) -> bool {
    match (mac_bytes(a), mac_bytes(b)) {
        (Some(a), Some(b)) => {
            let tail = |m: [u8; 6]| u16::from_be_bytes([m[4], m[5]]) as i32;
            a[..4] == b[..4] && (tail(a) - tail(b)).abs() <= 16
        }
        _ => false,
    }
}

/// From `brctl showmacs br0`: whether an address of the AP's device was seen
/// behind one of the LAN ports in the last FDB_RECENT seconds (a router that
/// is cabled in talks at least that often: ARP, 1905 discovery every 60 s).
/// `ports` maps port number to name.
pub fn fdb_cabled(showmacs: &str, ports: &std::collections::HashMap<u32, String>, ap: &str) -> bool {
    showmacs.lines().skip(1).any(|l| {
        // "port mac [l2_vlan] local ageing": this firmware's brctl adds a VLAN column.
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 4 {
            return false;
        }
        let (port, mac, local, age) = (f[0], f[1], f[f.len() - 2], f[f.len() - 1]);
        let on_lan = port.parse().ok().and_then(|n: u32| ports.get(&n)).is_some_and(|n| n.starts_with("eth"));
        local == "no" && on_lan && age.parse::<f32>().is_ok_and(|a| a < FDB_RECENT) && same_device(mac, ap)
    })
}

async fn cabled_to(ap: &str) -> bool {
    let mut ports = std::collections::HashMap::new();
    if let Ok(dir) = std::fs::read_dir("/sys/class/net/br0/brif") {
        for e in dir.flatten() {
            let no = std::fs::read_to_string(e.path().join("port_no")).unwrap_or_default();
            if let Ok(n) = u32::from_str_radix(no.trim().trim_start_matches("0x"), 16) {
                ports.insert(n, e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    let Ok(o) = crate::exec::run_raw("brctl", &["showmacs", "br0"], T).await else { return true };
    fdb_cabled(&o.stdout, &ports, ap)
}

/// The main router, for spotting it behind a LAN port: the AP the station
/// last joined, else the controller the 1905 daemon found (its AL MAC).
fn main_router() -> Option<String> {
    let last = LAST_AP.lock().map(|l| l.clone()).unwrap_or_default();
    if !last.is_empty() {
        return Some(last);
    }
    let log = std::fs::read(format!("{}/p1905.log", crate::mesh::RUN_DIR)).ok()?;
    crate::mesh::onboarding(&String::from_utf8_lossy(&log)).controller
}

async fn stations_off() {
    for st in STATIONS {
        let _ = crate::exec::run_raw("iwpriv", &[st, "set", "ApCliEnable=0"], T).await;
    }
}

/// Keep the Wi-Fi backhaul up, but never alongside a cable to the main router.
///
/// While the station is associated, a cable to the main router makes a loop
/// (the station's frames reach br0 even when it is not bridged), the kernel's
/// loop detection shuts that LAN port, and the main router disappears from it:
/// so the LAN ports are only conclusive while the station is off. On any sign
/// of a cable (a new link, a port shut by loop detection, the main router
/// behind a LAN port) the station is switched off, and it is (re)connected and
/// bridged only after the main router has been missing from the LAN ports for
/// a full FDB_RECENT window.
async fn supervise(gen: u64, uplink: Option<Uplink>) {
    let tick = std::time::Duration::from_secs(2);
    let mut carriers = lan_carriers().await;
    let mut idle_since = std::time::Instant::now();
    let mut announced = std::time::Instant::now();
    let mut wired_seen = std::time::Instant::now();
    while GEN.load(Ordering::SeqCst) == gen {
        hdswitch(false); // Wi-Fi reconfiguration may turn it back on
        let link = backhaul_link().await;
        let inbr = STATIONS.into_iter().find(|s| bridged(s));
        let now = lan_carriers().await;
        let plugged = now.iter().any(|p| !carriers.contains(p));
        carriers = now;
        let behind_lan = match main_router() {
            Some(r) => cabled_to(&r).await,
            None => false,
        };
        if plugged || loop_shut() || behind_lan {
            if let Some(b) = inbr {
                bridge_remove(b).await;
            }
            if link.is_some() {
                stations_off().await;
            }
            set_state(State::Cabled);
            wired_seen = std::time::Instant::now();
            if plugged {
                // Let the bridge learn who is behind the new cable.
                tokio::time::sleep(CABLE_SETTLE).await;
                carriers = lan_carriers().await;
            }
            tokio::time::sleep(tick).await;
            continue;
        }
        let clear = wired_seen.elapsed().as_secs_f32() >= FDB_RECENT;
        if link.is_some() {
            idle_since = std::time::Instant::now();
        }
        match (link, inbr) {
            (Some((st, ap)), None) => {
                if let Ok(mut l) = LAST_AP.lock() {
                    l.clone_from(&ap);
                }
                if !clear {
                    set_state(State::Connecting);
                } else if GEN.load(Ordering::SeqCst) == gen && bridge_add(st).await {
                    set_state(State::Bridged);
                    announce(3).await;
                    announced = std::time::Instant::now();
                }
            }
            (Some((st, _)), Some(b)) if st != b => bridge_remove(b).await,
            (Some(_), Some(_)) if announced.elapsed() > ANNOUNCE_EVERY => {
                announce(1).await;
                announced = std::time::Instant::now();
            }
            (None, Some(b)) => {
                bridge_remove(b).await;
                set_state(State::Connecting);
            }
            (None, None) => {
                set_state(State::Connecting);
                if let Some(up) = uplink.as_ref().filter(|_| clear && idle_since.elapsed() > RECONNECT_AFTER) {
                    connect_direct(up).await;
                    idle_since = std::time::Instant::now();
                }
            }
            _ => {}
        }
        tokio::time::sleep(tick).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cable_to_the_main_router_is_detected() {
        let ap = "02:00:00:00:e4:4f";
        assert!(same_device(ap, "02:00:00:00:e4:50") && same_device("02:00:00:00:e4:fe", "02:00:00:00:e5:02"));
        assert!(!same_device(ap, "02:00:00:01:e4:50") && !same_device(ap, "02:00:00:00:f0:00"));
        let ports = [(1, "eth0".to_string()), (7, "apcli1".to_string())].into_iter().collect();
        let fdb = |port: u32, local: &str, age: &str| {
            format!("port no mac addr                l2_vlan is local?       ageing timer\n  {port}     02:00:00:00:e4:50       -2              {local}                 {age}\n")
        };
        assert!(fdb_cabled(&fdb(1, "no", "0.42"), &ports, ap));
        assert!(!fdb_cabled(&fdb(1, "no", "200.00"), &ports, ap), "stale entry");
        assert!(!fdb_cabled(&fdb(7, "no", "0.42"), &ports, ap), "over the station");
        assert!(!fdb_cabled(&fdb(1, "yes", "0.00"), &ports, ap), "own address");
    }

    #[test]
    fn gratuitous_arp_layout() {
        let f = gratuitous_arp([2, 0, 0, 0, 0, 1], [192, 168, 1, 1]);
        assert_eq!(&f[..6], &[0xff; 6]);
        assert_eq!(&f[12..14], &[0x08, 0x06]);
        assert_eq!(&f[20..22], &[0, 1], "request");
        assert_eq!(&f[22..28], &[2, 0, 0, 0, 0, 1]);
        assert_eq!(&f[28..32], &[192, 168, 1, 1]);
        assert_eq!(&f[32..38], &[0; 6]);
        assert_eq!(&f[38..42], &[192, 168, 1, 1]);
    }

    #[test]
    fn ports_shut_by_loop_detection() {
        let block = |port: &str, flag: &str, status: &str| {
            format!("Port:                   {port}\nLoopback_en:            1\nPortdisloop_en:         1\nAlready_loop_flag:      {flag}\nPort_status:         {status}\n")
        };
        let head = "************Global Configure************ \ng_LoopbackEn:      1 \ng_Renew_time:      300s \n";
        let calm = format!("{head}{}{}{}", block("eth0", "0", ""), block("eth3", "0", "1"), block("wlan4", "0", "1"));
        assert!(loop_shut_ports(&calm).is_empty());
        let looped = format!("{head}{}{}", block("eth0", "0", "1"), block("eth3", "1", "0"));
        assert_eq!(loop_shut_ports(&looped), vec!["eth3".to_string()]);
        assert!(loop_shut_ports(&block("wlan0", "1", "0")).is_empty(), "Wi-Fi ports are not LAN cables");
    }
}
