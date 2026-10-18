//! Names of EasyMesh devices. 1905 topology carries no names, but the WSC
//! exchange that onboards a radio does: the controller's M2 and each agent's M1
//! include Device Name, Manufacturer, Model Name and Model Number (verified:
//! a TP-Link Archer AX12 sends "Archer AX12" / "TP-Link"). A passive listener
//! on br0 for 1905 frames (ethertype 0x893A only) records them by sender AL MAC.
//! Only those identity attributes are read; encrypted settings are skipped.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const ETH_P_1905: u16 = 0x893a;
const CMDU_AP_AUTOCONFIG_WSC: u16 = 0x0009;
const TLV_WSC: u8 = 0x11;
const STORE: &str = "/var/mesh/identities.json";

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Identity {
    pub device: String,
    pub manufacturer: String,
    pub model: String,
    pub number: String,
}

impl Identity {
    /// What to show for this device: its name, plus the vendor when the name
    /// does not already contain it.
    pub fn label(&self) -> String {
        let name = if !self.device.is_empty() { &self.device } else { &self.model };
        match (name.is_empty(), self.manufacturer.is_empty()) {
            (true, _) => self.manufacturer.clone(),
            (false, true) => name.clone(),
            (false, false) if name.to_lowercase().contains(&self.manufacturer.to_lowercase()) => name.clone(),
            (false, false) => format!("{name} · {}", self.manufacturer),
        }
    }
}

fn store() -> &'static Mutex<HashMap<String, Identity>> {
    static S: OnceLock<Mutex<HashMap<String, Identity>>> = OnceLock::new();
    S.get_or_init(|| {
        let saved = std::fs::read(STORE).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Mutex::new(saved)
    })
}

/// The recorded identity for an AL MAC, if any.
pub fn lookup(almac: &str) -> Option<Identity> {
    store().lock().ok()?.get(&almac.to_lowercase()).cloned()
}

/// The recorded device a MAC belongs to (an exact AL MAC, else one of the same
/// device's addresses, e.g. a BSSID next to its AL MAC), with that AL MAC.
pub fn lookup_device(mac: &str) -> Option<(String, Identity)> {
    let s = store().lock().ok()?;
    let mac = mac.to_lowercase();
    if let Some(i) = s.get(&mac) {
        return Some((mac, i.clone()));
    }
    let mut near: Vec<_> = s.iter().filter(|(k, _)| crate::backhaul::same_device(k, &mac)).collect();
    near.sort_by_key(|(k, _)| k.as_str());
    near.first().map(|(k, i)| (k.to_string(), (*i).clone()))
}

fn printable(v: &[u8]) -> String {
    String::from_utf8_lossy(v).chars().filter(|c| !c.is_control()).take(64).collect::<String>().trim().to_string()
}

/// Identity attributes of the WSC M1/M2 in an Ethernet frame, with the sender MAC.
pub fn parse_frame(f: &[u8]) -> Option<(String, Identity)> {
    if f.len() < 14 + 8 {
        return None;
    }
    let (mut et, mut c) = (u16::from_be_bytes([f[12], f[13]]), 14);
    if et == 0x8100 && f.len() > 18 + 8 {
        et = u16::from_be_bytes([f[16], f[17]]);
        c = 18;
    }
    if et != ETH_P_1905 || u16::from_be_bytes([f[c + 2], f[c + 3]]) != CMDU_AP_AUTOCONFIG_WSC {
        return None;
    }
    let src = f[6..12].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":");
    let mut i = c + 8;
    while i + 3 <= f.len() {
        let (t, l) = (f[i], u16::from_be_bytes([f[i + 1], f[i + 2]]) as usize);
        if t == 0 || i + 3 + l > f.len() {
            break;
        }
        if t == TLV_WSC {
            let (mut id, mut a, wsc) = (Identity::default(), 0, &f[i + 3..i + 3 + l]);
            while a + 4 <= wsc.len() {
                let (at, al) = (u16::from_be_bytes([wsc[a], wsc[a + 1]]), u16::from_be_bytes([wsc[a + 2], wsc[a + 3]]) as usize);
                let Some(v) = wsc.get(a + 4..a + 4 + al) else { break };
                match at {
                    0x1011 => id.device = printable(v),
                    0x1021 => id.manufacturer = printable(v),
                    0x1023 => id.model = printable(v),
                    0x1024 => id.number = printable(v),
                    _ => {} // everything else, encrypted settings included, is ignored
                }
                a += 4 + al;
            }
            if id != Identity::default() {
                return Some((src, id));
            }
        }
        i += 3 + l;
    }
    None
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn record(src: String, id: Identity) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(mut s) = store().lock() else { return };
    if s.get(&src) == Some(&id) {
        return;
    }
    s.insert(src, id);
    if let Ok(b) = serde_json::to_vec(&*s) {
        let _ = std::fs::create_dir_all("/var/mesh");
        if std::fs::write(STORE, b).is_ok() {
            let _ = std::fs::set_permissions(STORE, std::fs::Permissions::from_mode(0o600));
        }
    }
}

/// Start the passive listener on `ifname` (a background thread; Linux only).
#[cfg(target_os = "linux")]
pub fn spawn_listener(ifname: &'static str) {
    std::thread::spawn(move || {
        let proto = ETH_P_1905.to_be() as i32;
        // SAFETY: plain socket/bind/recv calls on a socket this thread owns.
        unsafe {
            let fd = libc::socket(libc::AF_PACKET, libc::SOCK_RAW, proto);
            if fd < 0 {
                eprintln!("meshid: socket failed");
                return;
            }
            let name = std::ffi::CString::new(ifname).expect("ifname");
            let mut sll: libc::sockaddr_ll = std::mem::zeroed();
            sll.sll_family = libc::AF_PACKET as u16;
            sll.sll_protocol = ETH_P_1905.to_be();
            sll.sll_ifindex = libc::if_nametoindex(name.as_ptr()) as i32;
            if libc::bind(fd, &sll as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_ll>() as u32) < 0 {
                eprintln!("meshid: bind {ifname} failed");
                libc::close(fd);
                return;
            }
            let mut buf = vec![0u8; 4096];
            loop {
                let n = libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0);
                if n <= 0 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
                if let Some((src, id)) = parse_frame(&buf[..n as usize]) {
                    record(src, id);
                }
            }
        }
    });
}

#[cfg(not(target_os = "linux"))]
pub fn spawn_listener(_ifname: &'static str) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr(t: u16, v: &[u8]) -> Vec<u8> {
        let mut a = t.to_be_bytes().to_vec();
        a.extend_from_slice(&(v.len() as u16).to_be_bytes());
        a.extend_from_slice(v);
        a
    }

    fn frame(wsc: Vec<u8>) -> Vec<u8> {
        let mut f = vec![0x01, 0x80, 0xc2, 0, 0, 0x13, 0x02, 0, 0, 0, 0x0c, 0x01, 0x89, 0x3a];
        f.extend_from_slice(&[0, 0, 0x00, 0x09, 0, 1, 0, 0x80]); // CMDU: AP-Autoconfiguration WSC
        f.push(TLV_WSC);
        f.extend_from_slice(&(wsc.len() as u16).to_be_bytes());
        f.extend(wsc);
        f.extend_from_slice(&[0, 0, 0]); // end of message
        f
    }

    #[test]
    fn m2_identity() {
        let mut w = attr(0x1022, &[5]);
        w.extend(attr(0x1011, b"Archer AX12"));
        w.extend(attr(0x1021, b"TP-Link"));
        w.extend(attr(0x1023, b"Archer AX12"));
        w.extend(attr(0x1024, b"1.0"));
        w.extend(attr(0x1018, &[0xde, 0xad])); // encrypted settings: ignored
        let (src, id) = parse_frame(&frame(w)).unwrap();
        assert_eq!(src, "02:00:00:00:0c:01");
        assert_eq!(id.device, "Archer AX12");
        assert_eq!(id.label(), "Archer AX12 · TP-Link");
    }

    #[test]
    fn other_frames_are_ignored() {
        let mut f = frame(attr(0x1011, b"x"));
        f[17] = 0x02; // a different CMDU type
        assert!(parse_frame(&f).is_none());
        assert!(parse_frame(&[0u8; 10]).is_none());
        let id = Identity { device: "TP-Link Deco".into(), manufacturer: "TP-Link".into(), ..Default::default() };
        assert_eq!(id.label(), "TP-Link Deco");
    }
}
