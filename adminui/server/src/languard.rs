//! Keeps the router's own address services off the LAN when they should be.
//!
//! - `DHCPSHostCfg.ServerEnable=0` is not honoured by this firmware: `dhcps`
//!   keeps answering on br0 after a reboot (verified 2026-10-17 with a probe
//!   DISCOVER: the ZTE offered a 192.168.1.x lease next to the main router).
//! - An EasyMesh agent must leave addressing to the main router, or clients on
//!   the agent's Wi-Fi get the ZTE's DHCP lease and IPv6 prefix.
//!
//! Both are enforced with firewall rules on traffic addressed to the router
//! itself; bridged traffic (clients talking to the main router) is untouched.
//! The rules are re-checked periodically because the firmware rebuilds parts
//! of its firewall on config changes. Nothing persists: a reboot clears them
//! and adminui puts them back.

use crate::state::App;
use std::sync::Arc;

const EVERY: std::time::Duration = std::time::Duration::from_secs(30);
const T: std::time::Duration = std::time::Duration::from_secs(10);

/// Requests to the router's DHCPv4 server.
const DHCP4: [&str; 9] = ["INPUT", "-i", "br0", "-p", "udp", "--dport", "67", "-j", "DROP"];
/// Requests to its DHCPv6 server, and its IPv6 router advertisements.
const DHCP6: [&str; 9] = ["INPUT", "-i", "br0", "-p", "udp", "--dport", "547", "-j", "DROP"];
const RA: [&str; 9] = ["OUTPUT", "-o", "br0", "-p", "icmpv6", "--icmpv6-type", "router-advertisement", "-j", "DROP"];

/// (block DHCPv4, block IPv6 addressing) for a DHCP server setting and mesh role.
pub fn wanted(server_enable: Option<&str>, mesh_agent: bool) -> (bool, bool) {
    (server_enable == Some("0") || mesh_agent, mesh_agent)
}

async fn ensure(tool: &str, rule: &[&str], want: bool) {
    let check: Vec<&str> = ["-C"].into_iter().chain(rule.iter().copied()).collect();
    let present = crate::exec::run_raw(tool, &check, T).await.is_ok_and(|o| o.ok());
    if want && !present {
        let add: Vec<&str> = ["-I", rule[0], "1"].into_iter().chain(rule[1..].iter().copied()).collect();
        if !crate::exec::run_raw(tool, &add, T).await.is_ok_and(|o| o.ok()) {
            eprintln!("languard: {tool} insert {} failed", rule.join(" "));
        }
    } else if !want && present {
        let del: Vec<&str> = ["-D"].into_iter().chain(rule.iter().copied()).collect();
        let _ = crate::exec::run_raw(tool, &del, T).await;
    }
}

/// Bring the rules in line with the current settings.
pub async fn sync(app: &App) {
    let server = app.db.read("DHCPSHostCfg").await.ok();
    let server_enable = server.as_ref().and_then(|t| t.row(0)).and_then(|r| r.get("ServerEnable"));
    let m = crate::mesh::Settings::load(&app.dir);
    let (v4, v6) = wanted(server_enable, m.enabled && m.role == crate::mesh::Role::Agent);
    ensure("iptables", &DHCP4, v4).await;
    ensure("ip6tables", &DHCP6, v6).await;
    ensure("ip6tables", &RA, v6).await;
}

pub async fn run(app: Arc<App>) {
    let mut iv = tokio::time::interval(EVERY);
    loop {
        iv.tick().await;
        sync(&app).await;
    }
}

#[cfg(test)]
mod tests {
    use super::wanted;

    #[test]
    fn what_is_blocked() {
        assert_eq!(wanted(Some("1"), false), (false, false));
        assert_eq!(wanted(Some("0"), false), (true, false), "server off is enforced");
        assert_eq!(wanted(Some("1"), true), (true, true), "a mesh agent leaves addressing to the controller");
        assert_eq!(wanted(None, false), (false, false), "unknown setting: leave it alone");
    }
}
