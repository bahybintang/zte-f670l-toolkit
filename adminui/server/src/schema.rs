//! The page schema (adminui/schema/pages/*.json, see SCHEMA.md), embedded at
//! build time. The server uses it as the write whitelist and for validation.

use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(rust_embed::RustEmbed)]
#[folder = "../schema/"]
#[include = "pages/*.json"]
#[include = "tables.txt"]
struct SchemaFiles;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub dm: String,
    #[serde(default)]
    pub label: String,
    #[serde(rename = "type", default = "default_type")]
    pub ty: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub pattern: Option<String>,
    pub on: Option<String>,
    pub off: Option<String>,
    #[serde(default)]
    pub options: Vec<Opt>,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub required: bool,
    pub live_key: Option<String>,
    #[serde(flatten)]
    pub rest: HashMap<String, Value>,
}

fn default_type() -> String {
    "string".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Opt {
    pub value: Value,
    #[serde(default)]
    pub label: String,
}

impl Opt {
    pub fn value_str(&self) -> String {
        match &self.value {
            Value::String(s) => s.clone(),
            v => v.to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    #[serde(default)]
    pub table: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    pub row: Option<Value>,
    pub row_map: Option<HashMap<String, Value>>,
    #[serde(default)]
    pub addable: bool,
    #[serde(default)]
    pub deletable: bool,
    pub max_rows: Option<u32>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(flatten)]
    pub rest: HashMap<String, Value>,
}

fn default_mode() -> String {
    "single".into()
}
fn default_source() -> String {
    "db".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub apply: Option<String>,
    #[serde(default)]
    pub danger: Option<String>,
    pub hook: Option<String>,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(flatten)]
    pub rest: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct PageFile {
    pages: Vec<Page>,
}

pub struct Schema {
    pub pages: Vec<Page>,
    pub by_id: HashMap<String, usize>,
    pub tables: Vec<String>,
    /// Raw JSON served to the web app.
    pub json: String,
}

impl Schema {
    pub fn load() -> Self {
        let mut pages = Vec::new();
        let mut names: Vec<_> = SchemaFiles::iter().filter(|n| n.ends_with(".json")).collect();
        names.sort();
        for n in names {
            let f = SchemaFiles::get(&n).unwrap();
            match serde_json::from_slice::<PageFile>(&f.data) {
                Ok(pf) => pages.extend(pf.pages),
                Err(e) => eprintln!("schema: skipping {n}: {e}"),
            }
        }
        // The server decides how a writable page applies: live when the Wi-Fi
        // hook or an OSS manager target covers every writable table, partly
        // live when only some are, otherwise at the next restart.
        for p in pages.iter_mut() {
            let w: Vec<&Section> =
                p.sections.iter().filter(|s| s.is_db() && s.mode != "readonly" && s.fields.iter().any(|f| !f.readonly)).collect();
            if w.is_empty() {
                continue;
            }
            // A writable section applies live when its manager takes a 0x2401
            // notify (oss::target) or it is a firewall rule list driven by the
            // register event (oss::is_fw_rule_table). Both are confirmed live
            // on device: the firewall re-reads a rule's match fields from the DB
            // after a 0x9703/0x9704 register (notes §9).
            let covered = w
                .iter()
                .filter(|s| crate::oss::target(&s.table).is_some() || crate::oss::is_fw_rule_table(&s.table))
                .count();
            p.apply = Some(
                if p.hook.as_deref() == Some("wifi") || covered == w.len() {
                    "live"
                } else if covered > 0 {
                    "unknown"
                } else {
                    "reboot"
                }
                .into(),
            );
        }
        let mut by_id = HashMap::new();
        for (i, p) in pages.iter().enumerate() {
            if by_id.insert(p.id.clone(), i).is_some() {
                eprintln!("schema: duplicate page id {}", p.id);
            }
        }
        let tables = SchemaFiles::get("tables.txt")
            .map(|f| String::from_utf8_lossy(&f.data).lines().map(str::to_string).collect())
            .unwrap_or_default();
        let json = serde_json::to_string(&serde_json::json!({ "pages": pages })).unwrap();
        Schema { pages, by_id, tables, json }
    }

    pub fn page(&self, id: &str) -> Option<&Page> {
        self.by_id.get(id).map(|&i| &self.pages[i])
    }
}

impl Section {
    pub fn is_db(&self) -> bool {
        self.source == "db" && !self.table.is_empty()
    }

    pub fn field(&self, dm: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.dm == dm)
    }

    /// Resolve which DB row a single-mode section edits for a selector value.
    pub fn resolve_row(&self, sel: Option<&str>) -> Option<u32> {
        let as_u32 = |v: &Value| match v {
            Value::Number(n) => n.as_u64().map(|n| n as u32),
            Value::String(s) => s.parse().ok(),
            _ => None,
        };
        let base: Option<String> = match &self.row {
            None => Some("0".into()),
            Some(Value::String(s)) if s == "$sel" => sel.map(str::to_string),
            Some(v) => as_u32(v).map(|n| n.to_string()),
        };
        let base = base?;
        if let Some(map) = &self.row_map {
            return map.get(&base).and_then(as_u32);
        }
        base.parse().ok()
    }
}

impl Field {
    pub fn is_secret(&self) -> bool {
        self.ty == "password"
    }

    /// Validate a value the client wants to write.
    pub fn validate(&self, v: &str) -> Result<(), String> {
        let name = if self.label.is_empty() { &self.dm } else { &self.label };
        if self.readonly {
            return Err(format!("{name} is read-only"));
        }
        crate::db::valid_value(v)?;
        let len = v.chars().count() as f64;
        match self.ty.as_str() {
            "bool" => {
                let on = self.on.as_deref().unwrap_or("1");
                let off = self.off.as_deref().unwrap_or("0");
                if v != on && v != off {
                    return Err(format!("{name}: expected {on} or {off}"));
                }
            }
            "int" => {
                let n: f64 = v.parse::<i64>().map_err(|_| format!("{name}: not an integer"))? as f64;
                if self.min.is_some_and(|m| n < m) || self.max.is_some_and(|m| n > m) {
                    return Err(format!("{name}: out of range"));
                }
            }
            "enum" => {
                if !self.options.is_empty() && !self.options.iter().any(|o| o.value_str() == v) {
                    return Err(format!("{name}: {v:?} is not an allowed option"));
                }
            }
            "ipv4" => {
                if !(v.is_empty() && !self.required) && v.parse::<std::net::Ipv4Addr>().is_err() {
                    return Err(format!("{name}: not an IPv4 address"));
                }
            }
            "ipv6" => {
                if !(v.is_empty() && !self.required) && v.parse::<std::net::Ipv6Addr>().is_err() {
                    return Err(format!("{name}: not an IPv6 address"));
                }
            }
            "mac" => {
                let ok = v.len() == 17
                    && v.split([':', '-']).count() == 6
                    && v.split([':', '-']).all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
                if !(v.is_empty() && !self.required) && !ok {
                    return Err(format!("{name}: not a MAC address"));
                }
            }
            _ => {
                if self.min.is_some_and(|m| len < m) || self.max.is_some_and(|m| len > m) {
                    return Err(format!("{name}: length must be {}–{}", self.min.unwrap_or(0.0), self.max.unwrap_or(1024.0)));
                }
            }
        }
        if self.required && v.is_empty() {
            return Err(format!("{name} is required"));
        }
        if let Some(p) = &self.pattern {
            if !v.is_empty() {
                if let Ok(re) = Regex::new(p) {
                    if !re.is_match(v) {
                        return Err(format!("{name}: invalid format"));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(ty: &str) -> Field {
        serde_json::from_value(serde_json::json!({"dm":"X","type":ty,"min":1,"max":5})).unwrap()
    }

    #[test]
    fn validates() {
        assert!(f("string").validate("abc").is_ok());
        assert!(f("string").validate("abcdef").is_err());
        assert!(f("int").validate("3").is_ok());
        assert!(f("int").validate("9").is_err());
        assert!(f("bool").validate("1").is_ok());
        assert!(f("bool").validate("yes").is_err());
        assert!(f("ipv4").validate("192.0.2.1").is_ok());
        assert!(f("mac").validate("00:11:22:33:44:5G").is_err());
    }

    #[test]
    fn rows() {
        let s: Section = serde_json::from_value(serde_json::json!({
            "id":"r","table":"WLANBase","row":"$sel","rowMap":{"0":0,"4":1}})).unwrap();
        assert_eq!(s.resolve_row(Some("4")), Some(1));
        assert_eq!(s.resolve_row(Some("2")), None);
    }

    #[test]
    fn embedded_schema_parses() {
        let s = Schema::load();
        assert!(!s.tables.is_empty());
        // Every page file must parse: a skipped file would silently drop pages.
        let files = SchemaFiles::iter().filter(|n| n.ends_with(".json")).count();
        for n in SchemaFiles::iter().filter(|n| n.ends_with(".json")) {
            let f = SchemaFiles::get(&n).unwrap();
            serde_json::from_slice::<PageFile>(&f.data).unwrap_or_else(|e| panic!("{n}: {e}"));
        }
        assert!(files > 0);
        for p in &s.pages {
            for sec in &p.sections {
                if sec.is_db() {
                    assert!(s.tables.contains(&sec.table), "{}: unknown table {}", p.id, sec.table);
                }
            }
        }
    }
}
