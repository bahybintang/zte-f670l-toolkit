//! Single admin user, Argon2id hash in adminui.conf, sessions in memory and
//! mirrored to tmpfs (they survive an adminui restart, not a reboot), CSRF
//! header on state-changing requests, per-IP login backoff.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const COOKIE: &str = "adminui_session";
const IDLE: Duration = Duration::from_secs(30 * 60);
const MAX_AGE: Duration = Duration::from_secs(12 * 3600);
const MAX_FAILS: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub user: String,
    pub pass_hash: String,
    #[serde(default)]
    pub default_password: bool,
    #[serde(default)]
    pub uboot_unlock: bool,
}

pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).expect("getrandom");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn argon() -> Argon2<'static> {
    // ~8 MiB, 2 passes: a few hundred ms on the router's Cortex-A9.
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(8 * 1024, 2, 1, None).unwrap())
}

pub fn hash_password(pw: &str) -> String {
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt).unwrap();
    let salt = SaltString::encode_b64(&salt).unwrap();
    argon().hash_password(pw.as_bytes(), &salt).unwrap().to_string()
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| argon().verify_password(pw.as_bytes(), &h).is_ok())
}

impl Config {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("adminui.conf")
    }

    pub fn load_or_init(dir: &Path) -> std::io::Result<Config> {
        let p = Self::path(dir);
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(c) = serde_json::from_str(&s) {
                return Ok(c);
            }
            eprintln!("adminui.conf unreadable; keeping it as .bad and re-initialising");
            let _ = std::fs::rename(&p, p.with_extension("conf.bad"));
        }
        let c = Config {
            user: "admin".into(),
            pass_hash: hash_password("admin"),
            default_password: true,
            uboot_unlock: false,
        };
        c.save(dir)?;
        Ok(c)
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let p = Self::path(dir);
        let tmp = p.with_extension("conf.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap())?;
        std::fs::rename(tmp, p)
    }
}

struct Session {
    created: Instant,
    last: Instant,
    csrf: String,
}

#[derive(Default)]
pub struct Auth {
    sessions: Mutex<HashMap<String, Session>>,
    fails: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    /// Where sessions are mirrored (tmpfs); None in tests.
    store: Option<PathBuf>,
}

/// Default session mirror: tmpfs, so a reboot still logs everyone out.
pub const SESSION_STORE: &str = "/var/run/adminui-sessions.json";
/// Activity is written back at most this often.
const SAVE_EVERY: Duration = Duration::from_secs(60);

/// A session on disk: ages, not timestamps (Instant is per-process, and the
/// router's wall clock resets at every boot).
#[derive(Serialize, Deserialize)]
struct Saved {
    csrf: String,
    age: u64,
    idle: u64,
}

pub struct SessionInfo {
    pub csrf: String,
}

impl Auth {
    /// Sessions mirrored to `path`, starting with whatever a previous run left.
    pub fn with_store(path: PathBuf) -> Self {
        let a = Auth { store: Some(path.clone()), ..Default::default() };
        if let Some(saved) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<HashMap<String, Saved>>(&b).ok()) {
            let now = Instant::now();
            let mut s = a.sessions.lock().unwrap();
            for (tok, v) in saved {
                let (Some(created), Some(last)) = (now.checked_sub(Duration::from_secs(v.age)), now.checked_sub(Duration::from_secs(v.idle))) else {
                    continue;
                };
                if v.idle < IDLE.as_secs() && v.age < MAX_AGE.as_secs() {
                    s.insert(tok, Session { created, last, csrf: v.csrf });
                }
            }
        }
        a
    }

    fn save(&self, s: &HashMap<String, Session>) {
        use std::os::unix::fs::PermissionsExt;
        let Some(path) = &self.store else { return };
        let out: HashMap<&String, Saved> = s
            .iter()
            .map(|(k, v)| (k, Saved { csrf: v.csrf.clone(), age: v.created.elapsed().as_secs(), idle: v.last.elapsed().as_secs() }))
            .collect();
        let tmp = path.with_extension("tmp");
        if let Ok(b) = serde_json::to_vec(&out) {
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    /// Returns Err(seconds to wait) when the IP is locked out.
    pub fn check_lockout(&self, ip: IpAddr) -> Result<(), u64> {
        let f = self.fails.lock().unwrap();
        if let Some((n, t)) = f.get(&ip) {
            if *n >= MAX_FAILS && t.elapsed() < LOCKOUT {
                return Err((LOCKOUT - t.elapsed()).as_secs() + 1);
            }
        }
        Ok(())
    }

    pub fn record_failure(&self, ip: IpAddr) {
        let mut f = self.fails.lock().unwrap();
        let e = f.entry(ip).or_insert((0, Instant::now()));
        if e.1.elapsed() > LOCKOUT {
            *e = (0, Instant::now());
        }
        e.0 += 1;
        e.1 = Instant::now();
    }

    pub fn clear_failures(&self, ip: IpAddr) {
        self.fails.lock().unwrap().remove(&ip);
    }

    pub fn create(&self) -> (String, String) {
        let token = random_hex(32);
        let csrf = random_hex(16);
        let now = Instant::now();
        let mut s = self.sessions.lock().unwrap();
        s.retain(|_, v| v.last.elapsed() < IDLE && v.created.elapsed() < MAX_AGE);
        s.insert(token.clone(), Session { created: now, last: now, csrf: csrf.clone() });
        self.save(&s);
        (token, csrf)
    }

    pub fn touch(&self, token: &str) -> Option<SessionInfo> {
        let mut s = self.sessions.lock().unwrap();
        let sess = s.get_mut(token)?;
        if sess.last.elapsed() > IDLE || sess.created.elapsed() > MAX_AGE {
            s.remove(token);
            self.save(&s);
            return None;
        }
        let stale = sess.last.elapsed() > SAVE_EVERY;
        sess.last = Instant::now();
        let info = SessionInfo { csrf: sess.csrf.clone() };
        if stale {
            self.save(&s);
        }
        Some(info)
    }

    pub fn remove(&self, token: &str) {
        let mut s = self.sessions.lock().unwrap();
        s.remove(token);
        self.save(&s);
    }
}

pub fn cookie_value<'a>(header: Option<&'a str>, name: &str) -> Option<&'a str> {
    header?.split(';').map(str::trim).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then_some(v)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip() {
        let h = hash_password("admin");
        assert!(verify_password("admin", &h));
        assert!(!verify_password("nope", &h));
    }

    #[test]
    fn cookies() {
        assert_eq!(cookie_value(Some("a=1; adminui_session=xyz"), COOKIE), Some("xyz"));
        assert_eq!(cookie_value(None, COOKIE), None);
    }

    #[test]
    fn sessions_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("adminui-sess-{}", random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.json");
        let a = Auth::with_store(path.clone());
        let (tok, csrf) = a.create();
        let b = Auth::with_store(path.clone());
        assert_eq!(b.touch(&tok).map(|i| i.csrf), Some(csrf), "restored after restart");
        b.remove(&tok);
        assert!(Auth::with_store(path).touch(&tok).is_none(), "logout persisted");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lockout() {
        let a = Auth::default();
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        for _ in 0..MAX_FAILS {
            a.record_failure(ip);
        }
        assert!(a.check_lockout(ip).is_err());
    }
}
