//! The apply pipeline: validate -> snapshot -> set+verify (rollback on any
//! mismatch) -> save -> live hook -> history. Plus undo and commit-confirm.

use crate::schema::{Field, Page, Schema, Section};
use crate::state::App;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use crate::db::USER_CFG;
const HISTORY_MAX: usize = 20;
const SNAPSHOTS_MAX: usize = 5;
pub const CONFIRM_WINDOW: Duration = Duration::from_secs(180);

#[derive(Debug, Deserialize)]
pub struct ChangeReq {
    pub section: String,
    pub row: Option<u32>,
    pub dm: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct AddReq {
    pub section: String,
    #[serde(default)]
    pub values: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct DelReq {
    pub section: String,
    pub row: u32,
}

#[derive(Debug, Deserialize, Default)]
pub struct ApplyReq {
    pub sel: Option<String>,
    #[serde(default)]
    pub changes: Vec<ChangeReq>,
    #[serde(default)]
    pub adds: Vec<AddReq>,
    #[serde(default)]
    pub deletes: Vec<DelReq>,
    /// Lockout guards the caller explicitly accepted (see LOCKOUT_GUARDS).
    #[serde(default)]
    pub acknowledge: Vec<String>,
}

/// Writes that remove a recovery path survive reboots (unlike the stock UI,
/// which never persists them), so they need an explicit acknowledgement.
const LOCKOUT_GUARDS: &[(&str, &str, &str, &str)] = &[
    ("TelnetCfg", "TS_Enable", "0", "telnet-off"),
    ("TelnetCfg", "Lan_Enable", "0", "telnet-off"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Applied {
    pub table: String,
    pub row: u32,
    pub dm: String,
    /// Redacted ("••••") for secrets when persisted.
    pub old: String,
    pub new: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: u64,
    pub ts: u64,
    pub page: String,
    pub title: String,
    pub changes: Vec<Applied>,
    /// Rows added/deleted: value-level undo is not possible, use the snapshot.
    pub structural: bool,
    pub snapshot: Option<String>,
    pub hook: Option<String>,
    pub undone: bool,
    pub undo_of: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub ok: bool,
    pub entry: Option<u64>,
    pub warnings: Vec<String>,
    pub confirm: Option<ConfirmInfo>,
    pub apply: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInfo {
    pub token: String,
    pub deadline_secs: u64,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub struct History {
    pub dir: PathBuf,
    pub entries: Vec<HistoryEntry>,
    /// Secret old values, kept in memory only: (entry, idx) -> value.
    secrets: HashMap<(u64, usize), String>,
    next_id: u64,
}

impl History {
    pub fn load(dir: PathBuf) -> Self {
        let entries: Vec<HistoryEntry> = std::fs::read_to_string(dir.join("history.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let _ = std::fs::create_dir_all(dir.join("backups"));
        History { dir, entries, secrets: HashMap::new(), next_id }
    }

    fn persist(&self) {
        let p = self.dir.join("history.json");
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_string(&self.entries).unwrap()).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }

    pub fn push(&mut self, mut e: HistoryEntry) -> u64 {
        e.id = self.next_id;
        self.next_id += 1;
        for (i, c) in e.changes.iter_mut().enumerate() {
            if c.secret {
                self.secrets.insert((e.id, i), std::mem::take(&mut c.old));
                c.old = "••••".into();
                c.new = "••••".into();
            }
        }
        let id = e.id;
        self.entries.push(e);
        while self.entries.len() > HISTORY_MAX {
            let old = self.entries.remove(0);
            self.secrets.retain(|(eid, _), _| *eid != old.id);
        }
        self.prune_snapshots();
        self.persist();
        id
    }

    fn prune_snapshots(&mut self) {
        let mut with: Vec<usize> = (0..self.entries.len()).filter(|&i| self.entries[i].snapshot.is_some()).collect();
        while with.len() > SNAPSHOTS_MAX {
            let i = with.remove(0);
            if let Some(s) = self.entries[i].snapshot.take() {
                let _ = std::fs::remove_file(self.dir.join("backups").join(s));
            }
        }
    }

    pub fn get(&self, id: u64) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn mark_undone(&mut self, id: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.undone = true;
        }
        self.persist();
    }

    /// Old values to restore for an undo, or Err if a secret was lost (restart).
    pub fn undo_values(&self, id: u64) -> Result<Vec<Applied>, String> {
        let e = self.get(id).ok_or("no such history entry")?;
        if e.undone {
            return Err("already undone".into());
        }
        if e.structural {
            return Err("this change added or removed rows; restore its snapshot instead".into());
        }
        let mut out = Vec::new();
        for (i, c) in e.changes.iter().enumerate() {
            let mut c = c.clone();
            if c.secret {
                c.old = self
                    .secrets
                    .get(&(e.id, i))
                    .cloned()
                    .ok_or("a password in this change is no longer known (adminui restarted); restore the snapshot instead")?;
            }
            out.push(c);
        }
        Ok(out)
    }
}

struct Planned<'a> {
    sec: &'a Section,
    field: &'a Field,
    row: u32,
    value: String,
}

fn section<'a>(page: &'a Page, id: &str) -> Result<&'a Section, String> {
    let s = page.sections.iter().find(|s| s.id == id).ok_or_else(|| format!("unknown section {id}"))?;
    if !s.is_db() {
        return Err(format!("section {id} is not writable"));
    }
    Ok(s)
}

fn plan<'a>(page: &'a Page, req: &ApplyReq) -> Result<Vec<Planned<'a>>, String> {
    let mut v = Vec::new();
    for c in &req.changes {
        let sec = section(page, &c.section)?;
        let field = sec.field(&c.dm).ok_or_else(|| format!("{} is not editable on this page", c.dm))?;
        field.validate(&c.value)?;
        let row = match sec.mode.as_str() {
            "list" => c.row.ok_or("row required for list sections")?,
            "readonly" => return Err(format!("section {} is read-only", sec.id)),
            _ => sec.resolve_row(req.sel.as_deref()).ok_or("cannot resolve row for this selection")?,
        };
        for (t, dm, val, id) in LOCKOUT_GUARDS {
            if sec.table == *t && field.dm == *dm && c.value == *val && !req.acknowledge.iter().any(|a| a == id) {
                return Err(format!("LOCKOUT:{id}: this turns off LAN telnet permanently, which is the recovery path if the web UI ever fails. Confirm to continue."));
            }
        }
        v.push(Planned { sec, field, row, value: c.value.clone() });
    }
    Ok(v)
}

async fn snapshot(app: &App, page: &str) -> Option<String> {
    let name = format!("{}-{}.xml", now(), page);
    let dst = app.dir.join("backups").join(&name);
    match tokio::fs::copy(USER_CFG, &dst).await {
        Ok(_) => Some(name),
        Err(e) => {
            eprintln!("snapshot failed: {e}");
            None
        }
    }
}

/// Set values with rollback. Caller holds the DB guard.
async fn set_all(app: &App, items: &[(String, u32, String, String)]) -> Result<Vec<usize>, (String, Vec<usize>)> {
    let mut done = Vec::new();
    for (i, (t, r, dm, val)) in items.iter().enumerate() {
        if let Err(e) = app.db.set_verified(t, *r, dm, val).await {
            return Err((e, done));
        }
        done.push(i);
    }
    Ok(done)
}

pub async fn apply(app: Arc<App>, schema: &Schema, page_id: &str, req: ApplyReq) -> Result<ApplyResult, String> {
    let page = schema.page(page_id).ok_or("unknown page")?;
    let planned = plan(page, &req)?;
    // Validate adds/deletes up front too.
    for a in &req.adds {
        let s = section(page, &a.section)?;
        if !s.addable {
            return Err(format!("rows cannot be added to {}", s.id));
        }
        for (dm, v) in &a.values {
            s.field(dm).ok_or_else(|| format!("{dm} is not editable"))?.validate(v)?;
        }
    }
    for d in &req.deletes {
        if !section(page, &d.section)?.deletable {
            return Err(format!("rows cannot be deleted from {}", d.section));
        }
    }
    if planned.is_empty() && req.adds.is_empty() && req.deletes.is_empty() {
        return Err("nothing to change".into());
    }

    let _g = app.db.exec.db_guard().await;
    let snap = snapshot(&app, page_id).await;

    // Old values, read fresh.
    let mut applied: Vec<Applied> = Vec::new();
    let mut items = Vec::new();
    for p in &planned {
        let t = app.db.read_fresh(&p.sec.table).await?;
        let old = t
            .row(p.row)
            .and_then(|r| r.get(&p.field.dm))
            .ok_or_else(|| format!("{}[{}].{} does not exist", p.sec.table, p.row, p.field.dm))?
            .to_string();
        if old == p.value {
            continue;
        }
        items.push((p.sec.table.clone(), p.row, p.field.dm.clone(), p.value.clone()));
        applied.push(Applied {
            table: p.sec.table.clone(),
            row: p.row,
            dm: p.field.dm.clone(),
            old,
            new: p.value.clone(),
            secret: p.field.is_secret(),
            live_key: p.field.live_key.clone(),
        });
    }

    if let Err((e, done)) = set_all(&app, &items).await {
        let back: Vec<_> = done.iter().rev().map(|&i| {
            let a = &applied[i];
            (a.table.clone(), a.row, a.dm.clone(), a.old.clone())
        }).collect();
        let rb = set_all(&app, &back).await;
        app.db.invalidate_all();
        return Err(match rb {
            Ok(_) => format!("{e} — all changes were rolled back"),
            Err((e2, _)) => format!("{e} — ROLLBACK ALSO FAILED: {e2}; restore the snapshot {}", snap.unwrap_or_default()),
        });
    }

    let mut warnings = Vec::new();
    let structural = !req.adds.is_empty() || !req.deletes.is_empty();
    // Deletes first (descending so row numbers stay valid), then adds.
    let mut dels: Vec<&DelReq> = req.deletes.iter().collect();
    dels.sort_by(|a, b| b.row.cmp(&a.row));
    for d in dels {
        let s = section(page, &d.section)?;
        app.db.delete_row(&s.table, d.row).await.map_err(|e| format!("delete failed: {e}"))?;
    }
    for a in &req.adds {
        let s = section(page, &a.section)?;
        let row = app.db.add_row(&s.table).await.map_err(|e| format!("add failed: {e}"))?;
        for (dm, v) in &a.values {
            if let Err(e) = app.db.set_verified(&s.table, row, dm, v).await {
                warnings.push(format!("new row {row}: {e}"));
            }
        }
    }

    app.db.save().await?;
    app.db.invalidate_all();

    if let Some(h) = &page.hook {
        warnings.extend(crate::hooks::run(&app, h, &applied).await);
    }
    drop(_g);

    let entry = HistoryEntry {
        id: 0,
        ts: now(),
        page: page_id.to_string(),
        title: page.title.clone(),
        changes: applied,
        structural,
        snapshot: snap,
        hook: page.hook.clone(),
        undone: false,
        undo_of: None,
    };
    let id = app.history.lock().await.push(entry);
    // Only the Wi-Fi hook applies live today; everything else is read by the
    // firmware at boot (raw DB writes send no change notification).
    let live = page.hook.is_some() && planned.iter().all(|p| p.field.live_key.is_some() || p.field.dm == "Enable");
    let apply_mode = if live { "live" } else { "reboot" };
    if !live {
        let mut rp = app.restart_pending.lock().unwrap();
        if !rp.contains(&page.title) {
            rp.push(page.title.clone());
        }
    }

    let confirm = if page.danger.as_deref() == Some("commit-confirm") && !structural && !live {
        // Not active until the next boot: arm the keep-or-revert window then.
        arm_on_boot(&app, id);
        warnings.push("This change takes effect after a restart. When the router comes back you have 3 minutes to keep it, or it is undone and the router restarts again.".into());
        None
    } else if page.danger.as_deref() == Some("commit-confirm") && !structural {
        Some(start_confirm(app.clone(), id, false).await)
    } else {
        if page.danger.as_deref() == Some("commit-confirm") {
            warnings.push("rows were added/removed: no automatic revert; use History → restore snapshot if needed".into());
        }
        None
    };
    Ok(ApplyResult { ok: true, entry: Some(id), warnings, confirm, apply: Some(apply_mode.into()) })
}

pub async fn undo(app: &App, id: u64) -> Result<u64, String> {
    let vals = app.history.lock().await.undo_values(id)?;
    let (page, title, hook) = {
        let h = app.history.lock().await;
        let e = h.get(id).unwrap();
        (e.page.clone(), e.title.clone(), e.hook.clone())
    };
    let _g = app.db.exec.db_guard().await;
    let items: Vec<_> = vals.iter().rev().map(|a| (a.table.clone(), a.row, a.dm.clone(), a.old.clone())).collect();
    set_all(app, &items).await.map_err(|(e, _)| format!("undo failed part-way: {e}"))?;
    app.db.save().await?;
    app.db.invalidate_all();
    let reverted: Vec<Applied> = vals
        .iter()
        .map(|a| Applied { old: a.new.clone(), new: a.old.clone(), ..a.clone() })
        .collect();
    if let Some(h) = &hook {
        for w in crate::hooks::run(app, h, &reverted).await {
            eprintln!("undo hook: {w}");
        }
    }
    drop(_g);
    let mut h = app.history.lock().await;
    h.mark_undone(id);
    Ok(h.push(HistoryEntry {
        id: 0,
        ts: now(),
        page,
        title: format!("Undo: {title}"),
        changes: reverted,
        structural: false,
        snapshot: None,
        hook,
        undone: false,
        undo_of: Some(id),
    }))
}

fn boot_file(app: &App) -> PathBuf {
    app.dir.join("confirm_on_boot.json")
}

fn arm_on_boot(app: &App, entry: u64) {
    let p = boot_file(app);
    let mut v: Vec<u64> = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if !v.contains(&entry) {
        v.push(entry);
    }
    let _ = std::fs::write(p, serde_json::to_string(&v).unwrap());
}

/// At startup: open the keep-or-revert window for changes applied before the reboot.
pub async fn resume_on_boot(app: Arc<App>) {
    let p = boot_file(&app);
    let Some(v) = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<Vec<u64>>(&s).ok()) else { return };
    let _ = std::fs::remove_file(&p);
    for entry in v {
        eprintln!("boot: commit-confirm window opened for history entry {entry}");
        start_confirm(app.clone(), entry, true).await;
    }
}

pub async fn pending_info(app: &App) -> Vec<serde_json::Value> {
    let p = app.pending.lock().await;
    let h = app.history.lock().await;
    p.iter()
        .map(|(tok, (entry, deadline))| {
            serde_json::json!({
                "token": tok, "entry": entry,
                "title": h.get(*entry).map(|e| e.title.clone()).unwrap_or_default(),
                "secondsLeft": deadline.saturating_duration_since(std::time::Instant::now()).as_secs(),
            })
        })
        .collect()
}

async fn start_confirm(app: Arc<App>, entry: u64, reboot_on_revert: bool) -> ConfirmInfo {
    let token = crate::auth::random_hex(12);
    let deadline = std::time::Instant::now() + CONFIRM_WINDOW;
    app.pending.lock().await.insert(token.clone(), (entry, deadline));
    let t = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(CONFIRM_WINDOW).await;
        let still = app.pending.lock().await.remove(&t).is_some();
        if still {
            eprintln!("commit-confirm {t} expired: reverting history entry {entry}");
            match undo(&app, entry).await {
                Ok(_) if reboot_on_revert => {
                    eprintln!("reverted; restarting so the boot-applied setting is undone too");
                    crate::exec::safe_reboot().await;
                }
                Ok(_) => {}
                Err(e) if reboot_on_revert => {
                    eprintln!("auto-revert by value failed ({e}); restoring the pre-change backup");
                    let snap = app.history.lock().await.get(entry).and_then(|e| e.snapshot.clone());
                    match snap {
                        Some(s) => {
                            if let Err(e) = restore_snapshot(&app, &s).await {
                                eprintln!("restore failed: {e}");
                            }
                        }
                        None => eprintln!("no backup for entry {entry}; change stays"),
                    }
                }
                Err(e) => eprintln!("auto-revert failed: {e}"),
            }
        }
    });
    ConfirmInfo { token, deadline_secs: CONFIRM_WINDOW.as_secs() }
}

pub async fn confirm(app: &App, token: &str) -> bool {
    app.pending.lock().await.remove(token).is_some()
}

/// Put a snapshot back as both the active and backup config, then reboot.
/// No `sidbg save` afterwards: that would overwrite the files from the running DB.
pub async fn restore_snapshot(app: &App, name: &str) -> Result<(), String> {
    if name.contains('/') || !name.ends_with(".xml") {
        return Err("bad backup name".into());
    }
    let src = app.dir.join("backups").join(name);
    let meta = tokio::fs::metadata(&src).await.map_err(|_| "backup file not found".to_string())?;
    if meta.len() < 1024 {
        return Err("backup file looks truncated".into());
    }
    let _g = app.db.exec.db_guard().await;
    for dst in [USER_CFG, "/userconfig/cfg/db_backup_cfg.xml"] {
        let tmp = format!("{dst}.adminui");
        tokio::fs::copy(&src, &tmp).await.map_err(|e| format!("copy: {e}"))?;
        tokio::fs::rename(&tmp, dst).await.map_err(|e| format!("rename: {e}"))?;
    }
    let _ = crate::exec::run_raw("sync", &[], Duration::from_secs(10)).await;
    eprintln!("restored {name}; rebooting");
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        crate::exec::safe_reboot().await;
    });
    Ok(())
}
