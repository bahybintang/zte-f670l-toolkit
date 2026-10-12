//! Runs device commands. Every call takes an argv (never a shell string built
//! from user input), has a timeout, and DB-touching calls are serialized: the
//! firmware's `sidbg` is not known to be safe under concurrent use, and a
//! write followed by its read-back must not interleave with another request.

use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

#[derive(Default)]
pub struct Executor {
    db_lock: Mutex<()>,
}

pub type ExecResult = Result<Output, String>;

impl Executor {
    /// Run a command that does not touch the config DB (stats, iwpriv reads...).
    pub async fn run(&self, prog: &str, args: &[&str], timeout: Duration) -> ExecResult {
        run_raw(prog, args, timeout).await
    }

    /// Hold the DB lock for a sequence of operations (set ... verify ... save).
    pub async fn db_guard(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.db_lock.lock().await
    }
}

pub async fn run_raw(prog: &str, args: &[&str], timeout: Duration) -> ExecResult {
    let child = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn {prog}: {e}"))?;
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(o)) => Ok(Output {
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            code: o.status.code(),
        }),
        Ok(Err(e)) => Err(format!("{prog}: {e}")),
        Err(_) => Err(format!("{prog}: timed out after {}s", timeout.as_secs())),
    }
}

/// Flush flash writes, then reboot. Never call `reboot` directly.
pub async fn safe_reboot() {
    let _ = run_raw("sync", &[], Duration::from_secs(15)).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _ = run_raw("reboot", &[], Duration::from_secs(10)).await;
}
