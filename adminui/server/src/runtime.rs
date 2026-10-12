//! Runtime (non-DB) data sources referenced by schema sections as
//! `"source": "runtime:<name>"`.

use crate::state::App;
use serde_json::{json, Value};

pub async fn read(app: &App, name: &str) -> Result<Value, String> {
    match name {
        "system" => Ok(serde_json::to_value(crate::status::system()).unwrap()),
        "arp" => Ok(json!({ "rows": crate::status::arp() })),
        "traffic" => Ok(serde_json::to_value(app.stats.traffic()).unwrap()),
        other => Err(format!("runtime source {other} not implemented yet")),
    }
}
