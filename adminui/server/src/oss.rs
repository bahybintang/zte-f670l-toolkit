//! Live apply through the firmware's own change notification: OSS event
//! 0x2401 sent to the manager module that owns a DB view, exactly as webd's
//! `pm_<x>_set_config` does after its DB write. No vendor library and no
//! shared memory: the view struct is rebuilt from `/etc/db_tbl_def.xml` and
//! the row values `sidbg` reads back. See notes/oss-protocol.md §2, §3, §8.
//!
//! Guard: a view is only sent when its computed length equals the length the
//! DB itself reports for that view (`sidbg 1 DB pv`). A mismatch means the
//! layout rule does not hold for that table, and nothing is sent.

use crate::db::Row;
use std::time::Duration;
use tokio::net::UnixDatagram;

pub const TBL_DEF: &str = "/etc/db_tbl_def.xml";
const EVENT_SET: u32 = 0x2401;
const SYN_REQ: u32 = 0x20000;
const VIEW_MAGIC: u32 = 0x0102_0304;
const SLOT_HDR: usize = 0x1c;
/// The DM that names a row of a multi-instance view (libdb dbViewCommOp).
const VIEW_NAME_DM: &str = "ViewName";
const REPLY_MAX: u16 = 2048;
const REPLY_TIMEOUT: Duration = Duration::from_secs(8);

/// A view whose manager takes a 0x2401 config notify. From libpmapi's
/// `pm_<x>_set_config` (notes/pm-targets.json).
pub struct Target {
    pub table: &'static str,
    pub view: u32,
    pub module: u32,
    pub subid: u32,
    /// The view name webd passes for a single-row view.
    pub identity: &'static str,
}

pub const TARGETS: &[Target] = &[
    Target { table: "SNTP", view: 250, module: 0x1010d, subid: 0x80001, identity: "IGD" },
    Target { table: "ARPDETECTConfig", view: 258, module: 0x101a6, subid: 0xe401, identity: "IGD" },
    Target { table: "IGMPProxy", view: 230, module: 0x10112, subid: 0xc0001, identity: "IGD" },
    Target { table: "MLDProxyCfg", view: 295, module: 0x1013a, subid: 0x280001, identity: "IGD" },
    Target { table: "DNSSettings", view: 177, module: 0x10116, subid: 0x120001, identity: "IGD" },
    Target { table: "FWALG", view: 237, module: 0x10102, subid: 0xb0002, identity: "IGD" },
    Target { table: "RIPConf", view: 263, module: 0x10127, subid: 0x220001, identity: "IGD" },
    // Multi-instance views: identity comes from each row's ViewName.
    Target { table: "DDNSService", view: 261, module: 0x10114, subid: 0xd0002, identity: "" },
    Target { table: "DNSHostsList", view: 179, module: 0x10116, subid: 0x120002, identity: "" },
    Target { table: "FWURL", view: 240, module: 0x10102, subid: 0xb000c, identity: "" },
    Target { table: "LoopbackEthConf", view: 191, module: 0x10140, subid: 0x340002, identity: "" },
    Target { table: "MLDWan", view: 294, module: 0x1013a, subid: 0x280001, identity: "" },
    Target { table: "PortControl", view: 186, module: 0x10102, subid: 0xb0015, identity: "" },
    Target { table: "PrefixCfg", view: 160, module: 0x10134, subid: 0x290004, identity: "" },
    Target { table: "QOSRule", view: 284, module: 0x10103, subid: 0x70002, identity: "" },
    Target { table: "QOSType", view: 285, module: 0x10103, subid: 0x70002, identity: "" },
    Target { table: "RIPIf", view: 264, module: 0x10127, subid: 0x220001, identity: "" },
    Target { table: "RaCfg", view: 311, module: 0x610101, subid: 0x300001, identity: "" },
    Target { table: "TUNNEL46CFG", view: 11, module: 0x1013b, subid: 0x310001, identity: "" },
    Target { table: "VLANBIND", view: 228, module: 0x101a3, subid: 0x480000, identity: "" },
    Target { table: "WLANBANDSTEERING", view: 382, module: 0x11201, subid: 0x10018, identity: "" },
    Target { table: "WLANROAMING", view: 381, module: 0x11201, subid: 0x10013, identity: "" },
    Target { table: "WLANWMM", view: 203, module: 0x11201, subid: 0x10007, identity: "" },
    Target { table: "WLANWEP", view: 204, module: 0x11201, subid: 0x10002, identity: "" },
    Target { table: "WLANWPS", view: 234, module: 0x11201, subid: 0x10004, identity: "" },
    Target { table: "QOSBasic", view: 277, module: 0x10103, subid: 0x70001, identity: "IGD" },
    Target { table: "QOSQueue", view: 282, module: 0x10103, subid: 0x70004, identity: "" },
    Target { table: "L3ForwardingRT", view: 7, module: 0x1010e, subid: 0x90001, identity: "" },
    Target { table: "DDNSClient", view: 260, module: 0x10114, subid: 0xd0001, identity: "" },
    Target { table: "DHCPSHostCfg", view: 172, module: 0x620101, subid: 0x150002, identity: "" },
    Target { table: "FWBase", view: 185, module: 0x10102, subid: 0xb0004, identity: "IGD" },
    Target { table: "FWLevel", view: 236, module: 0x10102, subid: 0xb0006, identity: "IGD" },
    Target { table: "SambaCfg", view: 144, module: 0x11401, subid: 0x260001, identity: "IGD" },
    Target { table: "DHCPSPool", view: 173, module: 0x620101, subid: 0x150003, identity: "" },
    Target { table: "DHCPSBind", view: 174, module: 0x620101, subid: 0x150001, identity: "" },
    Target { table: "LAND", view: 5, module: 0x1010b, subid: 0xa0001, identity: "" },
    Target { table: "DevAuthInfo", view: 29, module: 0x10111, subid: 0x110005, identity: "" },
    Target { table: "Log", view: 289, module: 0x10118, subid: 0x160001, identity: "IGD" },
    Target { table: "QOSPolicer", view: 290, module: 0x10103, subid: 0x70003, identity: "" },
    Target { table: "WANCPPP", view: 20, module: 0x10122, subid: 0x4a0001, identity: "" },
];

pub fn target(table: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|t| t.table == table)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Str(usize),
    Int32,
    Dword,
    Enum,
    Bool,
    Mac,
    Ip,
    Ipv6,
}

/// DB type name -> (type code, value encoding). Codes from cspd's type-name
/// table. IP is a host-order u32 (libcommfun `IPToStr` prints v>>24 first),
/// IPV6 the 16 address bytes in network order (`IPv6ToStr`). BUF16 is
/// pointer-backed in the DB and is not supported.
fn kind(t: &str) -> Option<(u16, Kind)> {
    if let Some(n) = t.strip_prefix("STR") {
        let n: usize = n.parse().ok()?;
        let code = [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096].iter().position(|&x| x == n)?;
        return Some((code as u16, Kind::Str(n)));
    }
    Some(match t {
        "INT32" => (10, Kind::Int32),
        "DWORD" => (11, Kind::Dword),
        "ENUM" => (12, Kind::Enum),
        "BOOL" => (13, Kind::Bool),
        "IP" => (14, Kind::Ip),
        "IPV6" => (15, Kind::Ipv6),
        "MAC" => (16, Kind::Mac),
        _ => return None,
    })
}

fn slot_len(k: Kind) -> usize {
    match k {
        Kind::Str(n) => 32 + n,
        Kind::Mac => 36,
        Kind::Ipv6 => 44,
        _ => 32,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dm {
    pub name: String,
    pub ty: String,
    /// The DMDEF's macroNeed (empty when unconditional). A gated DM is in the
    /// running view only when its macro is enabled on this build (ENABLED_MACROS).
    pub macro_need: String,
}

/// The `<DMDEF>`s of one table, in definition order.
pub fn table_dms(xml: &str, table: &str) -> Option<Vec<Dm>> {
    let start = xml.find(&format!("<Tbl name=\"{table}\""))?;
    let body = &xml[start..];
    let end = body.find("</TblDef>")?;
    let re = regex_lite::Regex::new(r#"<DMDEF\s+name="([^"]+)"[^>]*?\stype="([^"]+)"[^>]*?macroNeed="([^"]*)""#).ok()?;
    Some(
        re.captures_iter(&body[..end])
            .map(|c| Dm { name: c[1].to_string(), ty: c[2].to_string(), macro_need: c[3].to_string() })
            .collect(),
    )
}

/// Build macros enabled on this firmware (solved from every table's view
/// length against db_tbl_def.xml, 2026-10-13). A macro-gated DM is in the
/// running view only when its macroNeed is one of these.
const ENABLED_MACROS: &[&str] = &[
    "CONFIG_CTCC_SMART", "CONFIG_ZXICUS_ADEV_INFO", "CONFIG_ZXICUS_DHCPS_OPT125_E8",
    "CONFIG_ZXICUS_DNSSL", "CONFIG_ZXICUS_E8", "CONFIG_ZXICUS_IPV6", "CONFIG_ZXICUS_MULTI_E8",
    "CONFIG_ZXICUS_QOS_QC_OUTDEV", "CONFIG_ZXICUS_QOS_QC_TCREMARK", "CONFIG_ZXIC_YUEME3",
    "CONFIG_ZXIC_WLAN_WPS", "CONFIG_ZXICUS_IGMPSNOOPING", "CONFIG_ZXICUS_MLDSNOOPING",
    "CONFIG_ZXICUS_AUTOIP",
];

fn dm_in_build(d: &Dm) -> bool {
    if d.macro_need.is_empty() || ENABLED_MACROS.contains(&d.macro_need.as_str()) {
        return true;
    }
    // Runtime override for RE probing: ADMINUI_MACROS=comma,sep,list
    std::env::var("ADMINUI_MACROS").ok().is_some_and(|v| v.split(',').any(|m| m == d.macro_need))
}

/// Pick the DM layout whose view length matches the DB's. Preferred: the DMs
/// compiled into this build (macroNeed empty or enabled). Fallbacks, both still
/// length-checked: every DMDEF, or only the unconditional ones. Returns None
/// when none matches, so the caller falls back to reboot.
pub fn layout_for<'a>(dms: &'a [Dm], expect_len: usize) -> Option<Vec<&'a Dm>> {
    let built: Vec<&Dm> = dms.iter().filter(|d| dm_in_build(d)).collect();
    if view_len(&built) == Some(expect_len) {
        return Some(built);
    }
    let all: Vec<&Dm> = dms.iter().collect();
    if view_len(&all) == Some(expect_len) {
        return Some(all);
    }
    let ungated: Vec<&Dm> = dms.iter().filter(|d| d.macro_need.is_empty()).collect();
    (view_len(&ungated) == Some(expect_len)).then_some(ungated)
}

/// Total view length for a DM list, or None if a type is unsupported.
fn view_len(dms: &[&Dm]) -> Option<usize> {
    let mut n = 8;
    for d in dms {
        n += slot_len(kind(&d.ty)?.1);
    }
    Some(n)
}

/// The DB's own length for a view id, from `sidbg 1 DB pv` (ID, Dsc, RDsc, Len, Max).
pub fn pv_len(pv: &str, view: u32) -> Option<usize> {
    pv.lines().find_map(|l| {
        let c: Vec<&str> = l.split_whitespace().collect();
        (c.len() >= 4 && c[0].parse::<u32>().ok()? == view).then(|| c[3].parse().ok()).flatten()
    })
}

fn put_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn encode_value(k: Kind, v: &str, out: &mut [u8]) -> Result<(), String> {
    let num = |v: &str| -> Result<u32, String> {
        let v = v.trim();
        if v.is_empty() {
            return Ok(0);
        }
        v.parse::<u32>().or_else(|_| v.parse::<i32>().map(|x| x as u32)).map_err(|_| format!("not a number: {v:?}"))
    };
    match k {
        Kind::Str(n) => {
            if v.len() >= n {
                return Err(format!("value longer than {} bytes", n - 1));
            }
            out[..v.len()].copy_from_slice(v.as_bytes());
        }
        Kind::Int32 | Kind::Dword | Kind::Enum => out[..4].copy_from_slice(&num(v)?.to_le_bytes()),
        Kind::Bool => out[..4].copy_from_slice(&((num(v)? != 0) as u32).to_le_bytes()),
        Kind::Ip => {
            let a: u32 = if v.trim().is_empty() {
                0
            } else {
                v.trim().parse::<std::net::Ipv4Addr>().map_err(|_| format!("not an IPv4 address: {v:?}"))?.into()
            };
            out[..4].copy_from_slice(&a.to_le_bytes());
        }
        Kind::Ipv6 => {
            if !v.trim().is_empty() {
                let a: std::net::Ipv6Addr = v.trim().parse().map_err(|_| format!("not an IPv6 address: {v:?}"))?;
                out[..16].copy_from_slice(&a.octets());
            }
        }
        Kind::Mac => {
            let p: Vec<u8> = v.split(':').filter_map(|x| u8::from_str_radix(x, 16).ok()).collect();
            if p.len() != 6 {
                return Err(format!("not a MAC address: {v:?}"));
            }
            out[..6].copy_from_slice(&p);
        }
    }
    Ok(())
}

/// Build the view struct (8-byte header + one slot per DM) from a row.
/// `changed` marks the DMs this apply wrote, as webd's setters do.
pub fn build_view(dms: &[&Dm], row: &Row, changed: &[&str], expect_len: usize) -> Result<Vec<u8>, String> {
    let mut kinds = Vec::with_capacity(dms.len());
    for d in dms {
        let k = kind(&d.ty).ok_or_else(|| format!("{}: type {} not supported for live apply", d.name, d.ty))?;
        kinds.push(k);
    }
    let total = 8 + kinds.iter().map(|&(_, k)| slot_len(k)).sum::<usize>();
    if total != expect_len {
        return Err(format!("view layout mismatch: computed {total} bytes, DB says {expect_len}"));
    }
    let mut b = Vec::with_capacity(total);
    put_u32(&mut b, total as u32);
    put_u32(&mut b, VIEW_MAGIC);
    for (d, (code, k)) in dms.iter().zip(kinds) {
        let mut slot = vec![0u8; slot_len(k)];
        slot[0..2].copy_from_slice(&code.to_le_bytes());
        if changed.contains(&d.name.as_str()) {
            slot[2..4].copy_from_slice(&1u16.to_le_bytes());
        }
        // dbViewGetName: the view-name DM also carries its own name at +4.
        if d.name == VIEW_NAME_DM {
            let n = &d.name.as_bytes()[..d.name.len().min(15)];
            slot[4..4 + n.len()].copy_from_slice(n);
        }
        let v = row.get(&d.name).unwrap_or("");
        encode_value(k, v, &mut slot[SLOT_HDR..]).map_err(|e| format!("{}: {e}", d.name))?;
        b.extend_from_slice(&slot);
    }
    Ok(b)
}

/// Application message: sub id, view name (32, NUL-padded), data length, data.
pub fn mm_msg(subid: u32, identity: &str, view: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(0x28 + view.len());
    put_u32(&mut m, subid);
    let mut name = [0u8; 32];
    name[..identity.len().min(31)].copy_from_slice(&identity.as_bytes()[..identity.len().min(31)]);
    m.extend_from_slice(&name);
    put_u32(&mut m, view.len() as u32);
    m.extend_from_slice(view);
    m
}

/// 32-byte OSS header + payload (notes §2.2; 0x0E/0x10 corrected in §8).
pub fn datagram(sender: u32, receiver: u32, seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut d = vec![0u8; 32];
    d[0..4].copy_from_slice(&sender.to_le_bytes());
    d[4..8].copy_from_slice(&receiver.to_le_bytes());
    d[8..12].copy_from_slice(&(EVENT_SET | SYN_REQ).to_le_bytes());
    d[12..14].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    // 0x0E: largest reply we accept (the module refuses to reply if its answer
    // is longer); 0x10: the same, as the expected reply length. 0x16: request
    // counter, echoed at reply offset 6.
    d[14..16].copy_from_slice(&REPLY_MAX.to_le_bytes());
    d[16..18].copy_from_slice(&REPLY_MAX.to_le_bytes());
    d[22..24].copy_from_slice(&seq.to_le_bytes());
    d.extend_from_slice(payload);
    d
}

/// Module result from a reply datagram: event echo, length, seq, then iRet.
pub fn parse_reply(r: &[u8], seq: u16) -> Result<i32, String> {
    if r.len() < 12 {
        return Err(format!("short reply ({} bytes)", r.len()));
    }
    let ev = u16::from_le_bytes([r[0], r[1]]);
    let rs = u16::from_le_bytes([r[6], r[7]]);
    if ev as u32 != EVENT_SET || rs != seq {
        return Err(format!("unexpected reply event {ev:#x} seq {rs}"));
    }
    Ok(i32::from_le_bytes([r[8], r[9], r[10], r[11]]))
}

/// A process's inbound socket is named after the receiver's subsystem byte
/// (cspd hosts subsystem 0x01; other daemons have their own, e.g. 0x42).
pub fn request_sock(module: u32) -> String {
    format!("/var/tmp/0X{:08X}", (module >> 16) & 0xff)
}

/// Our reply address: subsystem 0x03 (not cspd's 0x01, so the module replies
/// over the socket), an AA/MM nobody has bound.
fn free_sender() -> Result<(u32, String), String> {
    for aa in 0x02u32..0x20 {
        let path = format!("/var/tmp/0X0000{:02X}{:02X}", 0x03, aa);
        if !std::path::Path::new(&path).exists() {
            return Ok(((0x03 << 16) | (aa << 8) | 0x01, path));
        }
    }
    Err("no free OSS reply address".into())
}

pub async fn send(t: &Target, payload: &[u8]) -> Result<i32, String> {
    let (sender, path) = free_sender()?;
    let sock = UnixDatagram::bind(&path).map_err(|e| format!("bind {path}: {e}"))?;
    // cspd runs as user csp; it must be able to write its reply to our socket.
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777));
    }
    let seq: u16 = 1;
    let r = async {
        sock.send_to(&datagram(sender, t.module, seq, payload), request_sock(t.module)).await.map_err(|e| format!("send: {e}"))?;
        let mut buf = vec![0u8; REPLY_MAX as usize + 8];
        let n = tokio::time::timeout(REPLY_TIMEOUT, sock.recv(&mut buf))
            .await
            .map_err(|_| "no reply from the manager".to_string())?
            .map_err(|e| format!("recv: {e}"))?;
        parse_reply(&buf[..n], seq)
    }
    .await;
    let _ = std::fs::remove_file(&path);
    r
}

/// Rebuild one row of `table`'s view from the DB and notify its manager.
/// `changed` names the DMs this apply wrote. A multi-instance row is named by
/// its own ViewName (e.g. IGD.RIPIf2), which is also the identity webd sends.
/// Returns the manager's iRet (0 = applied).
/// Probe an arbitrary (module, subid) with a table's view, for mapping which
/// manager owns a view. Returns the reply iRet, or an error.
pub async fn probe(db: &crate::db::Db, table: &str, row_no: u32, module: u32, subid: u32) -> Result<i32, String> {
    let t = Target { table: "", view: view_of(db, table).await?, module, subid, identity: "" };
    send_view(db, &t, table, row_no, &[]).await
}

async fn view_of(db: &crate::db::Db, table: &str) -> Result<u32, String> {
    // The table's view id: look it up by its VIEWFUNC szDsc in pv.
    let xml = tokio::fs::read_to_string(TBL_DEF).await.map_err(|e| format!("{TBL_DEF}: {e}"))?;
    let start = xml.find(&format!("<Tbl name=\"{table}\"")).ok_or_else(|| format!("{table} not in db_tbl_def"))?;
    let body = &xml[start..xml[start..].find("</Tbl>").map(|e| start + e).unwrap_or(xml.len())];
    let dsc = regex_lite::Regex::new(r#"szDsc="([^"]*)""#)
        .ok()
        .and_then(|re| re.captures(body).map(|c| c[1].to_string()))
        .unwrap_or_else(|| table.to_string());
    let pv = db.exec.run("sidbg", &["1", "DB", "pv"], Duration::from_secs(10)).await?;
    pv.stdout
        .lines()
        .find_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() >= 2 && c[1] == dsc).then(|| c[0].parse().ok()).flatten()
        })
        .ok_or_else(|| format!("no view for dsc {dsc}"))
}

async fn send_view(db: &crate::db::Db, t: &Target, table: &str, row_no: u32, changed: &[&str]) -> Result<i32, String> {
    let xml = tokio::fs::read_to_string(TBL_DEF).await.map_err(|e| format!("{TBL_DEF}: {e}"))?;
    let all_dms = table_dms(&xml, table).ok_or_else(|| format!("{table} not in {TBL_DEF}"))?;
    let pv = db.exec.run("sidbg", &["1", "DB", "pv"], Duration::from_secs(10)).await?;
    let len = pv_len(&pv.stdout, t.view).ok_or_else(|| format!("view {} not listed by the DB", t.view))?;
    let dms = layout_for(&all_dms, len).ok_or_else(|| format!("{table}: view layout does not match the DB length {len}"))?;
    let rows = db.read_fresh(table).await?;
    let row = rows.row(row_no).ok_or_else(|| format!("{table} has no row {row_no}"))?;
    let identity = if dms.iter().any(|d| d.name == VIEW_NAME_DM) {
        row.get(VIEW_NAME_DM).filter(|v| !v.is_empty() && v.len() < 32).ok_or_else(|| format!("{table} row {row_no} has no view name"))?
    } else {
        t.identity
    };
    let view = build_view(&dms, row, changed, len)?;
    send(t, &mm_msg(t.subid, identity, &view)).await
}

pub async fn reapply(db: &crate::db::Db, table: &str, row_no: u32, changed: &[&str]) -> Result<i32, String> {
    let t = target(table).ok_or_else(|| format!("{table} has no live-apply target"))?;
    send_view(db, t, table, row_no, changed).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<Tbl name="SNTP" RowCount="-1"><TblDef>
 <DMDEF name="Enable" dmId="0" type="BOOL" bKey="0" macroNeed=""/>
 <DMDEF name="NtpServer1" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="NtpServer2" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="NtpServer3" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="NtpServer4" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="NtpServer5" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="LocalTimeZone" dmId="0" type="STR16" bKey="0" macroNeed=""/>
 <DMDEF name="LocalTimeZoneName" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="DaylightSavingsUsed" dmId="0" type="BOOL" bKey="0" macroNeed=""/>
 <DMDEF name="DaylightSavingsStart" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="DaylightSavingsEnd" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="PollTimeInterval" dmId="0" type="DWORD" bKey="0" macroNeed=""/>
 <DMDEF name="isSynchronized" dmId="0" type="BOOL" bKey="0" macroNeed=""/>
 <DMDEF name="SupportAccessRefuse" dmId="0" type="BOOL" bKey="0" macroNeed=""/>
 <DMDEF name="Dscp" dmId="0" type="INT32" bKey="0" macroNeed=""/>
 <DMDEF name="QueueNum" dmId="0" type="INT32" bKey="0" macroNeed=""/>
 <DMDEF name="iZoneIndex" dmId="0" type="INT32" bKey="0" macroNeed=""/>
 <DMDEF name="sPosixTimeZoneName" dmId="0" type="STR64" bKey="0" macroNeed=""/>
 <DMDEF name="NTPServerType" dmId="0" type="INT32" bKey="0" macroNeed=""/>
</TblDef></Tbl>"#;

    fn row(f: &[(&str, &str)]) -> Row {
        Row { no: 0, fields: f.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }
    }

    #[test]
    fn sntp_view_matches_firmware_offsets() {
        let dms = table_dms(XML, "SNTP").unwrap();
        assert_eq!(dms.len(), 19);
        let lay = layout_for(&dms, 0x4b8).unwrap();
        let r = row(&[("NtpServer1", "pool.example"), ("PollTimeInterval", "86401"), ("Dscp", "-1"), ("Enable", "1")]);
        let v = build_view(&lay, &r, &["PollTimeInterval"], 0x4b8).unwrap();
        assert_eq!(v.len(), 0x4b8);
        assert_eq!(&v[4..8], &VIEW_MAGIC.to_le_bytes());
        // pm_sntp_get_config view offsets (+8 header): Enable 0, NtpServer1 32, Poll 880, Dscp 976.
        let at = |o: usize| 8 + o;
        assert_eq!(u16::from_le_bytes([v[at(0)], v[at(0) + 1]]), 13);
        assert_eq!(v[at(0) + 0x1c], 1);
        assert_eq!(&v[at(32) + 0x1c..at(32) + 0x1c + 12], b"pool.example");
        assert_eq!(u16::from_le_bytes([v[at(880)], v[at(880) + 1]]), 11);
        assert_eq!(u16::from_le_bytes([v[at(880) + 2], v[at(880) + 3]]), 1, "changed flag");
        assert_eq!(&v[at(880) + 0x1c..at(880) + 0x20], &86401u32.to_le_bytes());
        assert_eq!(&v[at(976) + 0x1c..at(976) + 0x20], &(-1i32).to_le_bytes());
        assert_eq!(mm_msg(0x80001, "IGD", &v).len(), 0x4e0);
    }

    #[test]
    fn ip_encodings() {
        let mut b = [0u8; 16];
        encode_value(Kind::Ip, "192.0.2.10", &mut b).unwrap();
        assert_eq!(&b[..4], &[10, 2, 0, 192]);
        let mut b = [0u8; 16];
        encode_value(Kind::Ipv6, "2001:db8::1", &mut b).unwrap();
        assert_eq!(b, [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert!(encode_value(Kind::Ip, "300.1.1.1", &mut [0u8; 4]).is_err());
        let mut z = [9u8; 4];
        encode_value(Kind::Ip, "", &mut z).unwrap();
        assert_eq!(z, [0; 4]);
    }

    #[test]
    fn view_name_slot() {
        let x = r#"<Tbl name="RIPIf"><TblDef>
 <DMDEF name="ViewName" dmId="0" type="STR32" bKey="1" macroNeed=""/>
 <DMDEF name="Enable" dmId="0" type="BOOL" bKey="0" macroNeed=""/>
</TblDef></Tbl>"#;
        let dms = table_dms(x, "RIPIf").unwrap();
        let lay: Vec<&Dm> = dms.iter().collect();
        let v = build_view(&lay, &row(&[("ViewName", "IGD.RIPIf2"), ("Enable", "1")]), &[], 8 + 64 + 32).unwrap();
        assert_eq!(u16::from_le_bytes([v[8], v[9]]), 2, "STR32 code");
        assert_eq!(&v[8 + 4..8 + 12], b"ViewName");
        assert_eq!(&v[8 + 0x1c..8 + 0x1c + 10], b"IGD.RIPIf2");
    }

    #[test]
    fn length_guard_refuses_mismatch() {
        let dms = table_dms(XML, "SNTP").unwrap();
        assert!(layout_for(&dms, 0x4b0).is_none());
        let lay: Vec<&Dm> = dms.iter().collect();
        assert!(build_view(&lay, &row(&[]), &[], 0x4b0).is_err());
    }

    #[test]
    fn layout_drops_macro_gated_dms_to_match_db() {
        let x = r#"<Tbl name="T"><TblDef>
 <DMDEF name="A" type="DWORD" macroNeed=""/>
 <DMDEF name="B" type="DWORD" macroNeed="CONFIG_X"/>
 <DMDEF name="C" type="BOOL" macroNeed=""/>
</TblDef></Tbl>"#;
        let dms = table_dms(x, "T").unwrap();
        // All three = 8 + 96; without the gated B = 8 + 64.
        assert_eq!(layout_for(&dms, 8 + 96).unwrap().len(), 3);
        assert_eq!(layout_for(&dms, 8 + 64).unwrap().len(), 2);
        assert!(layout_for(&dms, 8 + 50).is_none());
    }

    #[test]
    fn pv_and_reply() {
        let pv = "ID\tDsc\t\t\tRDsc\t\tLen\t\tMax\n250\tSNTP\t\tIGD\t\t1208\t\t0\n";
        assert_eq!(pv_len(pv, 250), Some(1208));
        assert_eq!(pv_len(pv, 251), None);
        let mut r = vec![0x01, 0x24, 0, 0, 8, 0, 1, 0];
        r.extend_from_slice(&0i32.to_le_bytes());
        assert_eq!(parse_reply(&r, 1), Ok(0));
        assert!(parse_reply(&r, 2).is_err());
        let d = datagram(0x30201, 0x1010d, 1, &[0; 0x4e0]);
        assert_eq!(d.len(), 32 + 0x4e0);
        assert_eq!(&d[8..12], &0x22401u32.to_le_bytes());
        assert_eq!(d[6], 0x01);
        assert_eq!(request_sock(0x1010d), "/var/tmp/0X00000001");
        assert_eq!(request_sock(0x420201), "/var/tmp/0X00000042");
    }
}
