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
    /// `sidbg` binds one fixed socket: a second concurrent instance fails with
    /// "create socket failed" and prints nothing else (verified on the device:
    /// 12 of 16 parallel reads failed). Every sidbg call takes this lock.
    sidbg_lock: Mutex<()>,
}

const SIDBG: &str = "sidbg";
const SOCKET_BUSY: &str = "create socket failed";
const SIDBG_RETRIES: u32 = 5;

pub type ExecResult = Result<Output, String>;

impl Executor {
    /// Run a command that does not touch the config DB (stats, iwpriv reads...).
    pub async fn run(&self, prog: &str, args: &[&str], timeout: Duration) -> ExecResult {
        if prog != SIDBG {
            return run_raw(prog, args, timeout).await;
        }
        let _g = self.sidbg_lock.lock().await;
        // A process outside adminui can still hold the socket briefly; retry.
        let mut attempt = 0;
        loop {
            let o = run_raw(prog, args, timeout).await?;
            attempt += 1;
            if !busy(&o) || attempt >= SIDBG_RETRIES {
                return Ok(o);
            }
            tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
        }
    }

    /// Hold the DB lock for a sequence of operations (set ... verify ... save).
    pub async fn db_guard(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.db_lock.lock().await
    }
}

/// sidbg lost the race for its socket (and produced no real output).
fn busy(o: &Output) -> bool {
    o.stdout.trim() == SOCKET_BUSY || (o.stdout.trim().is_empty() && o.stderr.contains(SOCKET_BUSY))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn out(stdout: &str, stderr: &str) -> Output {
        Output { stdout: stdout.into(), stderr: stderr.into(), code: Some(0) }
    }

    #[test]
    fn detects_socket_race() {
        assert!(busy(&out("create socket failed \n", "")));
        assert!(busy(&out("", "create socket failed")));
        assert!(!busy(&out("<Tbl name=\"WANC\" RowCount=\"1\">", "")));
    }
}
