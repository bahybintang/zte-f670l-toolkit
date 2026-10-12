//! Live-apply hooks run after a successful DB save. They make a change take
//! effect without a reboot. Failures are warnings: the DB already holds the
//! new value and boot-init would apply it after a restart.
//!
//! wifi: MediaTek RTPRIV driver via `iwpriv wlanN set Key=Value`.
//!   WLANCfg/WLANPSK/WLANWPS row r -> wlan{r}; WLANBase row 0 -> wlan0, row 1 -> wlan4.

use crate::apply::Applied;
use crate::state::App;
use std::time::Duration;

const T: Duration = Duration::from_secs(10);

/// Order matters for the driver: security first, SSID (which re-inits the
/// BSS) after it, PSK last-but-SSID per MediaTek convention.
fn key_rank(k: &str) -> u8 {
    match k {
        "AuthMode" => 0,
        "EncrypType" => 1,
        "WPAPSK" => 2,
        "SSID" => 3,
        _ => 4,
    }
}

fn iface_for(table: &str, row: u32) -> Option<String> {
    match table {
        "WLANBase" => match row {
            0 => Some("wlan0".into()),
            1 => Some("wlan4".into()),
            _ => None,
        },
        _ if row < 8 => Some(format!("wlan{row}")),
        _ => None,
    }
}

pub async fn run(app: &App, hook: &str, changes: &[Applied]) -> Vec<String> {
    match hook {
        "wifi" => wifi(app, changes).await,
        other => vec![format!("unknown hook {other}; change saved, takes effect after reboot")],
    }
}

async fn wifi(app: &App, changes: &[Applied]) -> Vec<String> {
    let mut warn = Vec::new();
    let mut cmds: Vec<(String, String, String)> = Vec::new(); // iface, key, value
    let mut updown: Vec<(String, bool)> = Vec::new();
    for c in changes {
        let Some(iface) = iface_for(&c.table, c.row) else { continue };
        if c.table == "WLANCfg" && c.dm == "Enable" {
            updown.push((iface, c.new == "1"));
            continue;
        }
        let Some(key) = &c.live_key else {
            warn.push(format!("{}: no live apply; takes effect after reboot", c.dm));
            continue;
        };
        cmds.push((iface, key.clone(), c.new.clone()));
    }
    cmds.sort_by_key(|(i, k, _)| (i.clone(), key_rank(k)));
    for (iface, key, val) in &cmds {
        let arg = format!("{key}={val}");
        match app.db.exec.run("iwpriv", &[iface, "set", &arg], T).await {
            Ok(o) if o.ok() => {}
            Ok(o) => warn.push(format!("iwpriv {iface} set {key}: {}", o.stderr.trim())),
            Err(e) => warn.push(format!("iwpriv {iface}: {e}")),
        }
    }
    for (iface, up) in updown {
        let r = app.db.exec.run("ifconfig", &[&iface, if up { "up" } else { "down" }], T).await;
        if !matches!(r, Ok(ref o) if o.ok()) {
            warn.push(format!("ifconfig {iface} failed"));
        }
    }
    warn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iface_map() {
        assert_eq!(iface_for("WLANCfg", 4).as_deref(), Some("wlan4"));
        assert_eq!(iface_for("WLANBase", 1).as_deref(), Some("wlan4"));
        assert_eq!(iface_for("WLANBase", 2), None);
    }
}
