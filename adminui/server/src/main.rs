//! adminui — on-device admin web UI for the ZTE F670L.
//!
//!   adminui serve [--dir D] [--bind IP] [--port N]   run the server
//!   adminui passwd [--dir D]                          reset the login to admin/admin
//!   adminui version

mod actions;
mod api;
mod apply;
mod assets;
mod auth;
mod db;
mod exec;
mod hooks;
mod runtime;
mod schema;
mod state;
mod status;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_DIR: &str = "/opt/upt/apps/adminui";
const DEFAULT_PORT: u16 = 8088;

struct Args {
    cmd: String,
    dir: PathBuf,
    bind: Option<IpAddr>,
    port: u16,
}

fn parse_args() -> Args {
    let mut a = Args { cmd: "serve".into(), dir: DEFAULT_DIR.into(), bind: None, port: DEFAULT_PORT };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--dir" => a.dir = it.next().expect("--dir needs a value").into(),
            "--bind" => a.bind = Some(it.next().expect("--bind needs a value").parse().expect("bad --bind")),
            "--port" => a.port = it.next().expect("--port needs a value").parse().expect("bad --port"),
            c if !c.starts_with('-') => a.cmd = c.to_string(),
            o => {
                eprintln!("unknown option {o}");
                std::process::exit(2);
            }
        }
    }
    a
}

/// The LAN bridge address: read from `ip -4 addr show br0`, falling back to 192.168.1.1.
fn lan_ip() -> IpAddr {
    let out = std::process::Command::new("ip").args(["-4", "-o", "addr", "show", "br0"]).output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(ip) = s.split_whitespace().skip_while(|w| *w != "inet").nth(1) {
            if let Ok(ip) = ip.split('/').next().unwrap_or("").parse() {
                return ip;
            }
        }
    }
    IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
}

fn main() {
    let args = parse_args();
    match args.cmd.as_str() {
        "version" => println!("adminui {}", env!("CARGO_PKG_VERSION")),
        "passwd" => {
            let _ = std::fs::create_dir_all(&args.dir);
            let _ = std::fs::remove_file(auth::Config::path(&args.dir));
            auth::Config::load_or_init(&args.dir).expect("write adminui.conf");
            println!("login reset to admin/admin (restart adminui to drop existing sessions)");
        }
        "serve" => serve(args),
        c => {
            eprintln!("unknown command {c}");
            std::process::exit(2);
        }
    }
}

fn serve(args: Args) {
    std::fs::create_dir_all(&args.dir).expect("create data dir");
    let cfg = auth::Config::load_or_init(&args.dir).expect("adminui.conf");
    let schema = Arc::new(schema::Schema::load());
    eprintln!("adminui {}: {} pages, {} tables", env!("CARGO_PKG_VERSION"), schema.pages.len(), schema.tables.len());
    let exec = Arc::new(exec::Executor::default());
    let app = Arc::new(state::App {
        dir: args.dir.clone(),
        db: db::Db::new(exec),
        auth: auth::Auth::default(),
        cfg: std::sync::Mutex::new(cfg),
        history: tokio::sync::Mutex::new(apply::History::load(args.dir.clone())),
        pending: Default::default(),
        schema,
        stats: Default::default(),
        restart_pending: Default::default(),
    });
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async move {
        apply::resume_on_boot(app.clone()).await;
        let s = app.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(status::SAMPLE_EVERY);
            loop {
                iv.tick().await;
                s.stats.sample();
            }
        });
        let ip = args.bind.unwrap_or_else(lan_ip);
        let addr = SocketAddr::new(ip, args.port);
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
            eprintln!("bind {addr}: {e}");
            std::process::exit(1);
        });
        eprintln!("listening on http://{addr}");
        let svc = api::router(app).into_make_service_with_connect_info::<SocketAddr>();
        axum::serve(listener, svc).await.unwrap();
    });
}
