use crate::apply::History;
use crate::auth::{Auth, Config};
use crate::db::Db;
use crate::schema::Schema;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct App {
    pub dir: PathBuf,
    pub db: Db,
    pub auth: Auth,
    pub cfg: std::sync::Mutex<Config>,
    pub history: Mutex<History>,
    /// commit-confirm token -> history entry
    pub pending: Mutex<HashMap<String, (u64, std::time::Instant)>>,
    pub schema: Arc<Schema>,
    pub stats: crate::status::Sampler,
    /// Titles of applied changes that only take effect after a restart.
    pub restart_pending: std::sync::Mutex<Vec<String>>,
}
