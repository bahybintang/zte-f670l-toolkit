//! WAN manual control: PPPoE connect/disconnect and DHCP renew/release, sent
//! to the connection managers exactly as the stock buttons do (oss::wan_manual).
//! The stock page only offers these on a routed connection, so a PPPoE row in
//! bridge mode or a WANCIP row with no WANC is refused here with a reason.
//! On the spare (no fiber, so no WAN interface ever comes up) the PPP manager
//! answers -641 in both bridge and routed mode and DHCP renew on an orphan row
//! answers -3; the meaning of -641 is not decoded (the manager's code is not in
//! the firmware extract), so it is reported as-is.

use crate::api::{R, S};
use crate::db::Table;
use crate::oss::WanCmd;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn routes() -> Router<Arc<crate::state::App>> {
    Router::new().route("/action/wan/state", get(state)).route("/action/wan/{id}", post(run))
}

/// WANC.ConnType for a routed connection (1 IP_Routed; 4 is PPPoE_Bridged).
const ROUTED: &str = "1";
/// WANCPPP.ConnTrigger 2 = Manual (0 Always on, 1 On demand).
const MANUAL: &str = "2";
/// WANCIP.Addressingtype 0 = DHCP (1 = static).
const DHCP: &str = "0";

/// One WAN connection a command can act on.
#[derive(Debug, Clone, PartialEq)]
pub struct Conn {
    pub view: String,
    pub name: String,
    pub connected: bool,
}

/// Which connection a command targets, or why none can be used.
pub fn pick(cmd: WanCmd, wanc: &Table, wancppp: &Table, wancip: &Table) -> Result<Conn, String> {
    let wanc_of = |view: &str| wanc.rows.iter().find(|r| r.get("ViewName") == Some(view));
    let label = |view: &str| {
        wanc_of(view).and_then(|r| r.get("WANCName")).filter(|n| !n.is_empty()).unwrap_or(view).to_string()
    };
    if cmd.is_ppp() {
        let mut bridged = None;
        for r in &wancppp.rows {
            let view = r.get("ViewName").unwrap_or("");
            let Some(c) = wanc_of(view) else { continue };
            if c.get("ConnType") == Some(ROUTED) {
                // si_ppp_manual_{connect,disconnect} refuse (-931) unless the dial mode is Manual.
                if r.get("ConnTrigger") != Some(MANUAL) {
                    return Err(format!(
                        "The PPPoE connection “{}” dials automatically (dial mode “{}”). Set the dial mode to Manual to connect and disconnect it by hand.",
                        label(view),
                        if r.get("ConnTrigger") == Some("1") { "On demand" } else { "Always on" }
                    ));
                }
                return Ok(Conn { view: view.into(), name: label(view), connected: c.get("Status").is_some_and(|s| s != "0") });
            }
            bridged.get_or_insert_with(|| label(view));
        }
        return Err(match bridged {
            Some(n) => format!("The PPPoE connection “{n}” is in bridge mode: the device behind the router dials it, not the router."),
            None => "No PPPoE connection is configured.".into(),
        });
    }
    let mut static_ip = None;
    for r in &wancip.rows {
        let view = r.get("ViewName").unwrap_or("");
        if wanc_of(view).is_none() {
            continue; // a leftover WANCIP row with no connection behind it
        }
        if r.get("Addressingtype") == Some(DHCP) {
            return Ok(Conn { view: view.into(), name: label(view), connected: r.get("ConnStatus") == Some("1") });
        }
        static_ip.get_or_insert_with(|| label(view));
    }
    Err(match static_ip {
        Some(n) => format!("The IPoE connection “{n}” uses a static address, so there is no DHCP lease."),
        None => "No IPoE (DHCP) connection is configured.".into(),
    })
}

/// Whether the command makes sense in the connection's current state.
pub fn applicable(cmd: WanCmd, c: &Conn) -> Result<(), &'static str> {
    match cmd {
        WanCmd::PppConnect if c.connected => Err("Already connected."),
        WanCmd::DhcpRelease if !c.connected => Err("No lease to release: the connection has no address."),
        _ => Ok(()),
    }
}

const IDS: [&str; 4] = ["ppp-connect", "ppp-disconnect", "dhcp-renew", "dhcp-release"];

async fn tables(app: &crate::state::App) -> Result<(Arc<Table>, Arc<Table>, Arc<Table>), String> {
    Ok((app.db.read_fresh("WANC").await?, app.db.read_fresh("WANCPPP").await?, app.db.read_fresh("WANCIP").await?))
}

/// Per action: whether it can run now, why not, and on which connection.
async fn state(State(app): S) -> R {
    let (wanc, ppp, ip) = tables(&app).await?;
    let mut actions = serde_json::Map::new();
    for id in IDS {
        let cmd = WanCmd::parse(id).expect("known id");
        let v = match pick(cmd, &wanc, &ppp, &ip) {
            Ok(c) => {
                let why = applicable(cmd, &c).err();
                json!({ "available": why.is_none(), "reason": why, "connection": c.name, "connected": c.connected })
            }
            Err(e) => json!({ "available": false, "reason": e }),
        };
        actions.insert(id.into(), v);
    }
    Ok(Json(json!({ "ok": true, "actions": Value::Object(actions) })))
}

async fn run(State(app): S, Path(id): Path<String>) -> R {
    let cmd = WanCmd::parse(&id).ok_or_else(|| format!("unknown WAN action '{id}'"))?;
    let (wanc, ppp, ip) = tables(&app).await?;
    let c = pick(cmd, &wanc, &ppp, &ip)?;
    applicable(cmd, &c)?;
    let code = crate::oss::wan_manual(&app.db, cmd, &c.view).await?;
    if code != 0 {
        let no_iface = wanc
            .rows
            .iter()
            .find(|r| r.get("ViewName") == Some(c.view.as_str()))
            .is_some_and(|r| r.get("RealIFName").unwrap_or("").is_empty());
        let hint = if no_iface { " The connection has no WAN interface yet (is the fiber link up?)." } else { "" };
        return Err(format!("The connection manager refused it for “{}” (code {code}).{hint}", c.name).into());
    }
    let what = match cmd {
        WanCmd::PppConnect => "Dialing",
        WanCmd::PppDisconnect => "Disconnecting",
        WanCmd::DhcpRenew => "Renewing the DHCP lease on",
        WanCmd::DhcpRelease => "Releasing the DHCP lease on",
    };
    Ok(Json(json!({ "ok": true, "message": format!("{what} “{}”. The status updates in a few seconds.", c.name) })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Row;

    fn t(rows: &[&[(&str, &str)]]) -> Table {
        Table {
            name: String::new(),
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, f)| Row { no: i as u32, fields: f.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() })
                .collect(),
        }
    }

    #[test]
    fn bridged_ppp_and_orphan_ip_are_refused() {
        // The spare unit's real shape: one bridged PPPoE WANC, one orphan WANCIP.
        let wanc = t(&[&[("ViewName", "IGD.WD1.WCD2.WCPPP1"), ("ConnType", "4"), ("WANCName", "2_INTERNET_B_VID_0"), ("Status", "0")]]);
        let ppp = t(&[&[("ViewName", "IGD.WD1.WCD2.WCPPP1")]]);
        let ip = t(&[&[("ViewName", "IGD.WD1.WCD1.WCIP1"), ("Addressingtype", "0")]]);
        assert!(pick(WanCmd::PppConnect, &wanc, &ppp, &ip).unwrap_err().contains("bridge mode"));
        assert!(pick(WanCmd::DhcpRenew, &wanc, &ppp, &ip).unwrap_err().contains("No IPoE"));
    }

    #[test]
    fn routed_connections_are_picked_with_live_state() {
        let wanc = t(&[
            &[("ViewName", "IGD.WD1.WCD1.WCPPP1"), ("ConnType", "1"), ("WANCName", "1_INTERNET_R_VID_10"), ("Status", "1")],
            &[("ViewName", "IGD.WD1.WCD2.WCIP1"), ("ConnType", "1"), ("WANCName", "2_TR069_R_VID_20")],
        ]);
        let ppp = t(&[&[("ViewName", "IGD.WD1.WCD1.WCPPP1"), ("ConnTrigger", "2")]]);
        let ip = t(&[&[("ViewName", "IGD.WD1.WCD2.WCIP1"), ("Addressingtype", "0"), ("ConnStatus", "0")]]);
        let p = pick(WanCmd::PppDisconnect, &wanc, &ppp, &ip).unwrap();
        assert_eq!((p.view.as_str(), p.name.as_str(), p.connected), ("IGD.WD1.WCD1.WCPPP1", "1_INTERNET_R_VID_10", true));
        assert_eq!(applicable(WanCmd::PppConnect, &p), Err("Already connected."));
        let d = pick(WanCmd::DhcpRelease, &wanc, &ppp, &ip).unwrap();
        assert!(!d.connected);
        assert!(applicable(WanCmd::DhcpRelease, &d).is_err());
        assert!(applicable(WanCmd::DhcpRenew, &d).is_ok());
    }

    #[test]
    fn always_on_ppp_is_refused() {
        let wanc = t(&[&[("ViewName", "V"), ("ConnType", "1"), ("WANCName", "P")]]);
        let ppp = t(&[&[("ViewName", "V"), ("ConnTrigger", "0")]]);
        assert!(pick(WanCmd::PppConnect, &wanc, &ppp, &t(&[])).unwrap_err().contains("Always on"));
    }

    #[test]
    fn static_ip_has_no_lease() {
        let wanc = t(&[&[("ViewName", "V"), ("ConnType", "1"), ("WANCName", "S")]]);
        let ip = t(&[&[("ViewName", "V"), ("Addressingtype", "1")]]);
        assert!(pick(WanCmd::DhcpRenew, &wanc, &t(&[]), &ip).unwrap_err().contains("static"));
    }
}
