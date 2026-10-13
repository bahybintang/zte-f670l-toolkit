use crate::apply::{self, ApplyReq};
use crate::auth::{self, COOKIE};
use crate::state::App;
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use std::net::SocketAddr;
use std::sync::Arc;

pub type S = State<Arc<App>>;

pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "ok": false, "error": self.1 }))).into_response()
    }
}

impl From<String> for ApiError {
    fn from(e: String) -> Self {
        ApiError(StatusCode::UNPROCESSABLE_ENTITY, e)
    }
}

impl From<&str> for ApiError {
    fn from(e: &str) -> Self {
        ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
    }
}

pub type R = Result<Json<Value>, ApiError>;

pub fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/session", get(session))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/password", post(change_password))
        .route("/schema", get(schema))
        .route("/page/{id}", get(page_read))
        .route("/page/{id}/apply", post(page_apply))
        .route("/confirm/{token}", post(confirm))
        .route("/pending", get(pending_list))
        .route("/history", get(history))
        .route("/history/{id}/undo", post(undo))
        .route("/history/{id}/restore", post(restore))
        .route("/status/system", get(status_system))
        .route("/status/traffic", get(status_traffic))
        .route("/status/leds", get(status_leds))
        .route("/status/live/{name}", get(status_live))
        .route("/db/tables", get(db_tables))
        .route("/db/table/{name}", get(db_table))
        .route("/db/set", post(db_set))
        .merge(crate::actions::routes())
        .layer(middleware::from_fn_with_state(app.clone(), guard));
    Router::new()
        .nest("/api", api)
        .fallback(crate::assets::serve)
        .with_state(app)
}

fn token_of(h: &HeaderMap) -> Option<String> {
    auth::cookie_value(h.get(header::COOKIE).and_then(|v| v.to_str().ok()), COOKIE).map(str::to_string)
}

/// Auth + CSRF + same-origin for everything under /api except session/login.
async fn guard(State(app): S, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let public = path == "/session" || path == "/login";
    let h = req.headers();
    // Same-origin: if the browser sent Origin, it must match Host.
    if let (Some(o), Some(host)) = (h.get(header::ORIGIN), h.get(header::HOST)) {
        let o = o.to_str().unwrap_or("");
        let host = host.to_str().unwrap_or("");
        if o.split("://").nth(1) != Some(host) {
            return ApiError(StatusCode::FORBIDDEN, "cross-origin request refused".into()).into_response();
        }
    }
    if !public {
        let Some(tok) = token_of(h) else {
            return ApiError(StatusCode::UNAUTHORIZED, "login required".into()).into_response();
        };
        let Some(sess) = app.auth.touch(&tok) else {
            return ApiError(StatusCode::UNAUTHORIZED, "session expired".into()).into_response();
        };
        if req.method() != Method::GET && req.method() != Method::HEAD {
            let sent = h.get("x-csrf-token").and_then(|v| v.to_str().ok()).unwrap_or("");
            if sent != sess.csrf {
                return ApiError(StatusCode::FORBIDDEN, "missing or bad CSRF token".into()).into_response();
            }
        }
    }
    let mut res = next.run(req).await;
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

async fn session(State(app): S, h: HeaderMap) -> Json<Value> {
    let cfg = app.cfg.lock().unwrap().clone();
    match token_of(&h).and_then(|t| app.auth.touch(&t)) {
        Some(s) => Json(json!({ "authenticated": true, "user": cfg.user, "csrf": s.csrf,
            "defaultPassword": cfg.default_password, "version": env!("CARGO_PKG_VERSION"),
            "restartPending": app.restart_pending.lock().unwrap().clone() })),
        None => Json(json!({ "authenticated": false })),
    }
}

#[derive(Deserialize)]
struct LoginReq {
    user: String,
    pass: String,
}

fn session_cookie(tok: &str, max_age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}={tok}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}")).unwrap()
}

async fn login(State(app): S, ConnectInfo(addr): ConnectInfo<SocketAddr>, Json(r): Json<LoginReq>) -> Result<Response, ApiError> {
    let ip = addr.ip();
    if let Err(wait) = app.auth.check_lockout(ip) {
        return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, format!("too many attempts; try again in {wait}s")));
    }
    let cfg = app.cfg.lock().unwrap().clone();
    let pass = r.pass.clone();
    let hash = cfg.pass_hash.clone();
    let ok = tokio::task::spawn_blocking(move || auth::verify_password(&pass, &hash)).await.unwrap_or(false);
    if !ok || r.user != cfg.user {
        app.auth.record_failure(ip);
        return Err(ApiError(StatusCode::UNAUTHORIZED, "wrong user name or password".into()));
    }
    app.auth.clear_failures(ip);
    let (tok, csrf) = app.auth.create();
    let mut res = Json(json!({ "ok": true, "csrf": csrf, "defaultPassword": cfg.default_password })).into_response();
    res.headers_mut().insert(header::SET_COOKIE, session_cookie(&tok, 12 * 3600));
    Ok(res)
}

async fn logout(State(app): S, h: HeaderMap) -> Response {
    if let Some(t) = token_of(&h) {
        app.auth.remove(&t);
    }
    let mut res = Json(json!({ "ok": true })).into_response();
    res.headers_mut().insert(header::SET_COOKIE, session_cookie("", 0));
    res
}

#[derive(Deserialize)]
struct PwReq {
    current: String,
    new: String,
}

async fn change_password(State(app): S, Json(r): Json<PwReq>) -> R {
    if r.new.chars().count() < 8 || r.new.len() > 128 {
        return Err("new password must be 8–128 characters".into());
    }
    let cfg = app.cfg.lock().unwrap().clone();
    let (cur, h) = (r.current.clone(), cfg.pass_hash.clone());
    if !tokio::task::spawn_blocking(move || auth::verify_password(&cur, &h)).await.unwrap_or(false) {
        return Err(ApiError(StatusCode::FORBIDDEN, "current password is wrong".into()));
    }
    let new = r.new.clone();
    let hash = tokio::task::spawn_blocking(move || auth::hash_password(&new)).await.map_err(|e| e.to_string())?;
    let mut c = app.cfg.lock().unwrap();
    c.pass_hash = hash;
    c.default_password = false;
    c.save(&app.dir).map_err(|e| e.to_string())?;
    Ok(Json(json!({ "ok": true })))
}

async fn schema(State(app): S) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], app.schema.json.clone()).into_response()
}

#[derive(Deserialize)]
struct SelQ {
    sel: Option<String>,
}

fn row_json(sec: &crate::schema::Section, r: &crate::db::Row) -> Value {
    let mut vals = Map::new();
    let mut set = Map::new();
    for f in &sec.fields {
        let v = r.get(&f.dm);
        if f.is_secret() {
            // Write-only: report only whether something is set.
            set.insert(f.dm.clone(), json!(v.is_some_and(|s| !s.is_empty())));
            vals.insert(f.dm.clone(), json!(""));
        } else if let Some(v) = v {
            vals.insert(f.dm.clone(), json!(v));
        }
    }
    json!({ "no": r.no, "values": vals, "secretSet": set })
}

async fn page_read(State(app): S, Path(id): Path<String>, Query(q): Query<SelQ>) -> R {
    let page = app.schema.page(&id).ok_or("unknown page")?;
    let mut out = Map::new();
    for sec in &page.sections {
        let v = if let Some(name) = sec.source.strip_prefix("runtime:") {
            crate::runtime::read(&app, name).await.unwrap_or_else(|e| json!({ "error": e }))
        } else if sec.is_db() {
            match app.db.read(&sec.table).await {
                Ok(t) => {
                    if sec.mode == "list" || sec.mode == "readonly" && sec.row.is_none() {
                        json!({ "rows": t.rows.iter().map(|r| row_json(sec, r)).collect::<Vec<_>>() })
                    } else {
                        match sec.resolve_row(q.sel.as_deref()) {
                            // A single-config row that doesn't exist yet (e.g. a
                            // feature not set up) shows an empty state, not an error.
                            Some(n) => match t.row(n) {
                                Some(r) => json!({ "rows": [row_json(sec, r)] }),
                                None => json!({ "rows": [] }),
                            },
                            None => json!({ "hidden": true }),
                        }
                    }
                }
                Err(e) => json!({ "error": e }),
            }
        } else {
            json!({})
        };
        out.insert(sec.id.clone(), v);
    }
    Ok(Json(json!({ "ok": true, "sections": out })))
}

async fn page_apply(State(app): S, Path(id): Path<String>, Json(r): Json<ApplyReq>) -> R {
    let schema = app.schema.clone();
    let res = apply::apply(app.clone(), &schema, &id, r).await?;
    Ok(Json(serde_json::to_value(res).unwrap()))
}

async fn confirm(State(app): S, Path(t): Path<String>) -> R {
    if apply::confirm(&app, &t).await {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err("no pending change with that token (already confirmed or reverted)".into())
    }
}

async fn pending_list(State(app): S) -> Json<Value> {
    Json(json!({ "ok": true, "pending": apply::pending_info(&app).await }))
}

async fn history(State(app): S) -> Json<Value> {
    let h = app.history.lock().await;
    let pending: Vec<u64> = app.pending.lock().await.values().map(|v| v.0).collect();
    let mut v: Vec<Value> = h.entries.iter().rev().map(|e| {
        let mut j = serde_json::to_value(e).unwrap();
        j["pendingConfirm"] = json!(pending.contains(&e.id));
        j
    }).collect();
    v.truncate(50);
    Json(json!({ "ok": true, "entries": v }))
}

async fn undo(State(app): S, Path(id): Path<u64>) -> R {
    // Undoing a pending commit-confirm entry also cancels its timer.
    app.pending.lock().await.retain(|_, e| e.0 != id);
    let new = apply::undo(&app, id).await?;
    Ok(Json(json!({ "ok": true, "entry": new })))
}

async fn restore(State(app): S, Path(id): Path<u64>, Json(c): Json<serde_json::Value>) -> R {
    if c["confirm"] != "RESTORE" {
        return Err("type RESTORE to confirm".into());
    }
    let snap = app.history.lock().await.get(id).and_then(|e| e.snapshot.clone()).ok_or("this change has no backup any more")?;
    apply::restore_snapshot(&app, &snap).await?;
    Ok(Json(json!({ "ok": true, "message": "backup restored; the router is restarting" })))
}

async fn status_system(State(app): S) -> Json<Value> {
    let mut v = serde_json::to_value(crate::status::system()).unwrap();
    if let Ok(t) = app.db.read("DevInfo").await {
        if let Some(r) = t.row(0) {
            for k in ["ModelName", "SoftwareVer", "HardwareVer", "SerialNumber", "ManuFacturer", "BootVer"] {
                if let Some(x) = r.get(k) {
                    v[k] = json!(x);
                }
            }
        }
    }
    Json(json!({ "ok": true, "system": v }))
}

async fn status_traffic(State(app): S) -> Json<Value> {
    Json(json!({ "ok": true, "traffic": app.stats.traffic() }))
}

async fn status_leds(State(app): S) -> Json<Value> {
    Json(json!({ "ok": true, "leds": crate::runtime::leds(&app).await }))
}

async fn status_live(State(app): S, Path(name): Path<String>) -> R {
    Ok(Json(json!({ "ok": true, "data": crate::runtime::read(&app, &name).await? })))
}

async fn db_tables(State(app): S) -> Json<Value> {
    Json(json!({ "ok": true, "tables": app.schema.tables }))
}

async fn db_table(State(app): S, Path(name): Path<String>) -> R {
    let t = app.db.read_fresh(&name).await?;
    // Raw editor: values are shown as stored, except well-known secret DMs.
    let secret = |dm: &str| {
        let d = dm.to_ascii_lowercase();
        d.contains("password") || d.contains("passphrase") || d.contains("presharedkey") || d.ends_with("pwd") || d.contains("secret") || d.starts_with("wepkey") || d == "key"
    };
    let rows: Vec<Value> = t.rows.iter().map(|r| {
        let f: Vec<Value> = r.fields.iter().map(|(k, v)| {
            if secret(k) { json!([k, if v.is_empty() { "" } else { "••••" }, true]) } else { json!([k, v, false]) }
        }).collect();
        json!({ "no": r.no, "fields": f })
    }).collect();
    Ok(Json(json!({ "ok": true, "name": t.name, "rows": rows })))
}

#[derive(Deserialize)]
struct RawSet {
    table: String,
    row: u32,
    dm: String,
    value: String,
}

/// Raw DB editor write: same snapshot/verify/save/history discipline, no hook.
async fn db_set(State(app): S, Json(r): Json<RawSet>) -> R {
    let _g = app.db.exec.db_guard().await;
    let t = app.db.read_fresh(&r.table).await?;
    let old = t.row(r.row).and_then(|x| x.get(&r.dm)).ok_or("no such row/field")?.to_string();
    let name = format!("{}-raw-{}.xml", apply::now(), r.table);
    let snap = tokio::fs::copy(apply::USER_CFG, app.dir.join("backups").join(&name)).await.ok().map(|_| name);
    app.db.set_verified(&r.table, r.row, &r.dm, &r.value).await?;
    app.db.save().await?;
    drop(_g);
    let lower = r.dm.to_ascii_lowercase();
    let secret = lower.contains("pass") || lower.contains("key") || lower.contains("secret") || lower.contains("pwd");
    let id = app.history.lock().await.push(apply::HistoryEntry {
        id: 0,
        ts: apply::now(),
        page: "raw-db".into(),
        title: format!("Raw edit {}[{}].{}", r.table, r.row, r.dm),
        changes: vec![apply::Applied { table: r.table, row: r.row, dm: r.dm, old, new: r.value, secret, live_key: None }],
        structural: false,
        snapshot: snap,
        hook: None,
        undone: false,
        undo_of: None,
    });
    Ok(Json(json!({ "ok": true, "entry": id, "apply": "save" })))
}
