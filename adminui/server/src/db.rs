//! Config DB access through the firmware's own debug tool:
//!   sidbg 1 DB p <Tbl>                  print table
//!   sidbg 1 DB set <Tbl> <row> <DM> <v> write running DB
//!   sidbg 1 DB save                     persist to /userconfig/cfg
//!   sidbg 1 DB addr <Tbl> / delr <Tbl> <row>
//! `sidbg` exits 0 even on failure, so every write is verified by read-back.

use crate::exec::{Executor, ExecResult};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SIDBG: &str = "sidbg";
const CACHE_TTL: Duration = Duration::from_secs(2);
const T: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub no: u32,
    /// DM name -> value, in table order.
    pub fields: Vec<(String, String)>,
}

impl Row {
    pub fn get(&self, dm: &str) -> Option<&str> {
        self.fields.iter().find(|(k, _)| k == dm).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Table {
    pub name: String,
    pub rows: Vec<Row>,
}

impl Table {
    pub fn row(&self, no: u32) -> Option<&Row> {
        self.rows.iter().find(|r| r.no == no)
    }
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Parse `sidbg 1 DB p` output. Line oriented: values may contain quotes.
pub fn parse_table(out: &str) -> Result<Table, String> {
    let mut name = None;
    let mut rows: Vec<Row> = Vec::new();
    for line in out.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("<Tbl name=\"") {
            name = rest.split('"').next().map(str::to_string);
        } else if let Some(rest) = l.strip_prefix("<Row No=\"") {
            let no = rest.split('"').next().and_then(|n| n.parse().ok()).ok_or("bad Row")?;
            rows.push(Row { no, fields: Vec::new() });
        } else if let Some(rest) = l.strip_prefix("<DM name=\"") {
            let Some((dm, after)) = rest.split_once("\" val=\"") else { continue };
            let val = after.strip_suffix("\"/>").unwrap_or(after);
            if let Some(r) = rows.last_mut() {
                r.fields.push((dm.to_string(), unescape(val)));
            }
        }
    }
    match name {
        Some(name) => Ok(Table { name, rows }),
        None => {
            let msg = out.lines().find(|l| !l.trim().is_empty()).unwrap_or("no output");
            Err(format!("table not found: {msg}"))
        }
    }
}

/// Table and DM names come from the schema or the raw editor; restrict them to
/// the character set the firmware uses so they can never look like options.
pub fn valid_ident(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.starts_with('-')
}

pub fn valid_value(s: &str) -> Result<(), String> {
    if s.len() > 1024 {
        return Err("value too long".into());
    }
    if s.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
        return Err("value contains a newline or NUL".into());
    }
    Ok(())
}

pub struct Db {
    pub exec: Arc<Executor>,
    cache: Mutex<HashMap<String, (Instant, Arc<Table>)>>,
}

impl Db {
    pub fn new(exec: Arc<Executor>) -> Self {
        Self { exec, cache: Mutex::new(HashMap::new()) }
    }

    async fn sidbg(&self, args: &[&str]) -> ExecResult {
        let mut a = vec!["1", "DB"];
        a.extend_from_slice(args);
        self.exec.run(SIDBG, &a, T).await
    }

    pub fn invalidate(&self, table: &str) {
        self.cache.lock().unwrap().remove(table);
    }

    pub fn invalidate_all(&self) {
        self.cache.lock().unwrap().clear();
    }

    pub async fn read(&self, table: &str) -> Result<Arc<Table>, String> {
        if !valid_ident(table) {
            return Err(format!("invalid table name {table:?}"));
        }
        if let Some((t, tbl)) = self.cache.lock().unwrap().get(table) {
            if t.elapsed() < CACHE_TTL {
                return Ok(tbl.clone());
            }
        }
        self.read_fresh(table).await
    }

    pub async fn read_fresh(&self, table: &str) -> Result<Arc<Table>, String> {
        if !valid_ident(table) {
            return Err(format!("invalid table name {table:?}"));
        }
        let out = self.sidbg(&["p", table]).await?;
        let tbl = Arc::new(parse_table(&out.stdout)?);
        self.cache.lock().unwrap().insert(table.to_string(), (Instant::now(), tbl.clone()));
        Ok(tbl)
    }

    /// Caller must hold `exec.db_guard()`. Writes, then verifies by read-back.
    pub async fn set_verified(&self, table: &str, row: u32, dm: &str, val: &str) -> Result<(), String> {
        if !valid_ident(table) || !valid_ident(dm) {
            return Err("invalid table/field name".into());
        }
        valid_value(val)?;
        let row_s = row.to_string();
        self.sidbg(&["set", table, &row_s, dm, val]).await?;
        self.invalidate(table);
        let t = self.read_fresh(table).await?;
        let got = t.row(row).and_then(|r| r.get(dm));
        match got {
            Some(g) if g == val => Ok(()),
            Some(g) => Err(format!("{table}[{row}].{dm}: wrote {val:?}, read back {g:?}")),
            None => Err(format!("{table}[{row}].{dm}: field or row does not exist")),
        }
    }

    /// Caller must hold `exec.db_guard()`.
    pub async fn save(&self) -> Result<(), String> {
        let o = self.sidbg(&["save"]).await?;
        if o.stdout.to_ascii_lowercase().contains("fail") {
            return Err(format!("save failed: {}", o.stdout.trim()));
        }
        Ok(())
    }

    /// Caller must hold `exec.db_guard()`. Returns the new row number.
    pub async fn add_row(&self, table: &str) -> Result<u32, String> {
        let before = self.read_fresh(table).await?;
        let known: Vec<u32> = before.rows.iter().map(|r| r.no).collect();
        self.sidbg(&["addr", table]).await?;
        self.invalidate(table);
        let after = self.read_fresh(table).await?;
        after
            .rows
            .iter()
            .map(|r| r.no)
            .find(|n| !known.contains(n))
            .ok_or_else(|| format!("{table}: addr did not create a row (table full?)"))
    }

    /// Caller must hold `exec.db_guard()`.
    pub async fn delete_row(&self, table: &str, row: u32) -> Result<(), String> {
        let before = self.read_fresh(table).await?;
        let n = before.rows.len();
        let row_s = row.to_string();
        self.sidbg(&["delr", table, &row_s]).await?;
        self.invalidate(table);
        let after = self.read_fresh(table).await?;
        if after.rows.len() >= n {
            return Err(format!("{table}: delr {row} removed nothing"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_table() {
        let out = "<Tbl name=\"SNTP\" RowCount=\"1\">\n\t<Row No=\"0\">\n\t\t<DM name=\"Enable\" val=\"1\"/>\n\t\t<DM name=\"Q\" val=\"a\"b&amp;\"/>\n\t</Row>\n</Tbl>\n";
        let t = parse_table(out).unwrap();
        assert_eq!(t.name, "SNTP");
        assert_eq!(t.rows[0].get("Enable"), Some("1"));
        assert_eq!(t.rows[0].get("Q"), Some("a\"b&"));
    }

    #[test]
    fn missing_table() {
        assert!(parse_table("table(X) not find,and there is no similar table\n").is_err());
    }

    #[test]
    fn idents() {
        assert!(valid_ident("WLANCfg"));
        assert!(valid_ident("11iAuthMode"));
        assert!(!valid_ident("-x"));
        assert!(!valid_ident("a b"));
        assert!(valid_value("x\ny").is_err());
    }
}
