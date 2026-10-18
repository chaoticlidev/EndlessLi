//! Python sidecar lifecycle and JSON-RPC client (line-delimited stdio).
//!
//! Rust owns the sidecar: lazy start on first request, health checks,
//! restart with a cap, kill on exit. In dev the sidecar runs from the
//! project venv (`sidecar/.venv`); release builds spawn the bundled binary
//! next to the app executable. Status changes broadcast through an event
//! channel that the shell forwards as `sidecar://status` Tauri events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use el_core::{AppError, AppResult};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};

/// Consecutive crash count before AI features are disabled.
pub const MAX_RESTARTS: u32 = 3;

/// Restart window: the crash counter resets after this many quiet seconds.
pub const RESTART_WINDOW_SECS: u64 = 300;

/// Per-call timeout for sidecar requests.
pub const CALL_TIMEOUT_SECS: u64 = 90;

/// Health-check timeout.
pub const HEALTH_TIMEOUT_SECS: u64 = 10;

/// Lifecycle state of the Python sidecar process.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SidecarStatus {
    /// Never started (lazy start on first request).
    #[default]
    Stopped,
    /// Process is running and passed the last health check.
    Running,
    /// Restarting after a crash.
    Restarting,
    /// Disabled after [`MAX_RESTARTS`] failures; diagnostics surfaced to UI.
    Disabled,
}

/// A JSON-RPC error from the sidecar.
#[derive(Debug)]
pub struct SidecarRpcError {
    pub code: i64,
    pub message: String,
}

/// How to launch the sidecar, resolved once at first use.
#[derive(Debug, Clone)]
enum LaunchPlan {
    /// Bundled release binary.
    Binary(PathBuf),
    /// Dev interpreter plus working directory (package import root).
    Python {
        interpreter: PathBuf,
        workdir: PathBuf,
    },
}

/// Owns the sidecar child process and multiplexes JSON-RPC calls over its
/// stdio. Cloneable; all state is internally synchronized.
#[derive(Debug, Clone)]
pub struct SidecarManager {
    inner: Arc<ManagerInner>,
}

#[derive(Debug)]
struct ManagerInner {
    restarts: AtomicU32,
    last_crash_at: Mutex<Option<std::time::Instant>>,
    status: Mutex<SidecarStatus>,
    next_id: AtomicU64,
    child: Mutex<Option<Child>>,
    // Async mutex: this guard is held across writes and flushes.
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, SidecarRpcError>>>>,
    status_tx: mpsc::UnboundedSender<SidecarStatus>,
    // Where the sidecar caches downloaded model weights (persistent).
    model_cache_dir: PathBuf,
}

impl SidecarManager {
    /// Creates a stopped manager. `status_tx` receives every status change;
    /// `model_cache_dir` is exported to the sidecar as its embedding cache.
    pub fn new(status_tx: mpsc::UnboundedSender<SidecarStatus>, model_cache_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(ManagerInner {
                restarts: AtomicU32::new(0),
                last_crash_at: Mutex::new(None),
                status: Mutex::new(SidecarStatus::Stopped),
                next_id: AtomicU64::new(1),
                child: Mutex::new(None),
                stdin: tokio::sync::Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                status_tx,
                model_cache_dir,
            }),
        }
    }

    /// Current lifecycle status.
    pub fn status(&self) -> SidecarStatus {
        *self.inner.status.lock().expect("sidecar status lock")
    }

    /// Liveness probe: starts the sidecar if needed and asks for a version.
    pub async fn health(&self) -> AppResult<String> {
        self.ensure_running().await?;
        let value = self
            .raw_call("health.check", Value::Null, HEALTH_TIMEOUT_SECS)
            .await?;
        value
            .get("version")
            .and_then(|version| version.as_str())
            .map(str::to_string)
            .ok_or_else(|| AppError::Internal("sidecar health response has no version".to_string()))
    }

    /// Sends one JSON-RPC request and resolves with the result.
    pub async fn call(&self, method: &str, params: Value) -> AppResult<Value> {
        self.ensure_running().await?;
        self.raw_call(method, params, CALL_TIMEOUT_SECS).await
    }

    /// Sends one request over an already-running sidecar. Split from the
    /// public entry points so startup health checks do not recurse.
    async fn raw_call(&self, method: &str, params: Value, timeout_secs: u64) -> AppResult<Value> {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.inner.pending.lock().expect("pending lock");
            pending.insert(id, tx);
        }
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let write_result = {
            let mut stdin_guard = self.inner.stdin.lock().await;
            let stdin = stdin_guard
                .as_mut()
                .ok_or_else(|| AppError::Internal("sidecar stdin is gone".to_string()))?;
            let payload = serde_json::to_string(&line)
                .map_err(|err| AppError::Internal(format!("encode sidecar request: {err}")))?;
            stdin
                .write_all(format!("{payload}\n").as_bytes())
                .await
                .map_err(|err| AppError::Internal(format!("write to sidecar: {err}")))?;
            stdin
                .flush()
                .await
                .map_err(|err| AppError::Internal(format!("flush sidecar: {err}")))
        };
        if let Err(err) = write_result {
            self.inner.pending.lock().expect("pending lock").remove(&id);
            self.on_transport_failure().await;
            return Err(err);
        }
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(rpc))) => Err(AppError::Internal(format!(
                "sidecar {} error {}: {}",
                method, rpc.code, rpc.message
            ))),
            Ok(Err(_)) => {
                self.on_transport_failure().await;
                Err(AppError::Internal("sidecar request cancelled".to_string()))
            }
            Err(_) => {
                self.inner.pending.lock().expect("pending lock").remove(&id);
                Err(AppError::Internal(format!("sidecar {method} timed out")))
            }
        }
    }

    /// Starts the sidecar when it is not running.
    async fn ensure_running(&self) -> AppResult<()> {
        if self.is_alive() {
            return Ok(());
        }
        self.spawn().await
    }

    fn is_alive(&self) -> bool {
        let mut child_guard = self.inner.child.lock().expect("child lock");
        match child_guard.as_mut() {
            Some(child) => child.try_wait().map(|exit| exit.is_none()).unwrap_or(false),
            None => false,
        }
    }

    fn set_status(&self, status: SidecarStatus) {
        *self.inner.status.lock().expect("sidecar status lock") = status;
        let _ = self.inner.status_tx.send(status);
    }

    async fn spawn(&self) -> AppResult<()> {
        // Reset the crash counter outside the restart window.
        let quiet = self
            .inner
            .last_crash_at
            .lock()
            .expect("crash lock")
            .is_none_or(|at| at.elapsed().as_secs() > RESTART_WINDOW_SECS);
        if quiet {
            self.inner.restarts.store(0, Ordering::SeqCst);
        }
        if self.inner.restarts.load(Ordering::SeqCst) >= MAX_RESTARTS {
            self.set_status(SidecarStatus::Disabled);
            return Err(AppError::Internal(
                "sidecar disabled after repeated crashes; restart the app to retry".to_string(),
            ));
        }
        self.set_status(SidecarStatus::Restarting);
        let plan = resolve_launch().map_err(AppError::Internal)?;
        let mut command = match &plan {
            LaunchPlan::Binary(path) => {
                let mut command = Command::new(path);
                command
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true);
                command
            }
            LaunchPlan::Python {
                interpreter,
                workdir,
            } => {
                let mut command = Command::new(interpreter);
                command
                    .arg("-u")
                    .arg("-m")
                    .arg("endlessli_sidecar")
                    .current_dir(workdir)
                    .env("PYTHONPATH", workdir)
                    .env("PYTHONUNBUFFERED", "1")
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true);
                command
            }
        };
        // Both launch plans cache model weights in the app data directory
        // (fastembed otherwise defaults to the OS temp dir, which is not
        // durable) and keep the Hugging Face symlink warning out of the log.
        command
            .env("ENDLESSLI_EMBED_CACHE_DIR", &self.inner.model_cache_dir)
            .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1");
        let mut child = command
            .spawn()
            .map_err(|err| AppError::Internal(format!("spawn sidecar ({plan:?}): {err}")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Internal("sidecar stdout was not piped".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Internal("sidecar stderr was not piped".to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Internal("sidecar stdin was not piped".to_string()))?;
        *self.inner.child.lock().expect("child lock") = Some(child);
        *self.inner.stdin.lock().await = Some(stdin);
        self.set_status(SidecarStatus::Running);
        self.pump_stdout(stdout);
        self.pump_stderr(stderr);
        // Fail fast when the interpreter is broken instead of timing out later.
        // Uses the raw call: the full health probe would recurse into startup.
        match self
            .raw_call("health.check", Value::Null, HEALTH_TIMEOUT_SECS)
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => {
                self.on_transport_failure().await;
                Err(err)
            }
        }
    }

    /// Handles a dead pipe or vanished child: records the crash so the next
    /// call restarts (or disables after too many crashes).
    async fn on_transport_failure(&self) {
        *self.inner.child.lock().expect("child lock") = None;
        *self.inner.stdin.lock().await = None;
        self.inner.pending.lock().expect("pending lock").clear();
        *self.inner.last_crash_at.lock().expect("crash lock") = Some(std::time::Instant::now());
        let restarts = self.inner.restarts.fetch_add(1, Ordering::SeqCst) + 1;
        if restarts >= MAX_RESTARTS {
            self.set_status(SidecarStatus::Disabled);
        } else {
            self.set_status(SidecarStatus::Restarting);
        }
    }

    /// Records a crash without transport context (kept for the restart-cap
    /// unit test and manual failure injection).
    pub fn record_crash(&self) -> SidecarStatus {
        let restarts = self.inner.restarts.fetch_add(1, Ordering::SeqCst) + 1;
        let next = if restarts >= MAX_RESTARTS {
            SidecarStatus::Disabled
        } else {
            SidecarStatus::Restarting
        };
        self.set_status(next);
        next
    }

    fn pump_stdout(&self, stdout: tokio::process::ChildStdout) {
        let pending_owner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let response: Value = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(err) => {
                        tracing::warn!("sidecar sent invalid JSON: {err}");
                        continue;
                    }
                };
                let id = response.get("id").and_then(Value::as_u64);
                let sender = id.and_then(|id| {
                    pending_owner
                        .pending
                        .lock()
                        .expect("pending lock")
                        .remove(&id)
                });
                if let Some(sender) = sender {
                    if let Some(error) = response.get("error") {
                        let _ = sender.send(Err(SidecarRpcError {
                            code: error
                                .get("code")
                                .and_then(|code| code.as_i64())
                                .unwrap_or(-1),
                            message: error
                                .get("message")
                                .and_then(|message| message.as_str())
                                .unwrap_or("unknown sidecar error")
                                .to_string(),
                        }));
                    } else {
                        let _ =
                            sender.send(Ok(response.get("result").cloned().unwrap_or(Value::Null)));
                    }
                }
            }
            tracing::warn!("sidecar stdout closed");
        });
    }

    fn pump_stderr(&self, stderr: tokio::process::ChildStderr) {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    tracing::warn!(target: "sidecar", "{line}");
                }
            }
        });
    }
}

/// Resolves how to launch the sidecar: an explicit directory, the bundled
/// release binary next to the executable, or the dev venv.
fn resolve_launch() -> Result<LaunchPlan, String> {
    if let Ok(dir) = std::env::var("ENDLESSLI_SIDECAR_DIR") {
        return resolve_dev_dir(Path::new(&dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["endlessli-sidecar.exe", "endlessli-sidecar"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Ok(LaunchPlan::Binary(candidate));
                }
            }
        }
    }
    for base in [std::env::current_dir().map_err(|err| format!("read cwd: {err}"))?] {
        for candidate in [base.join("sidecar"), base.join("../sidecar")] {
            if candidate.join("endlessli_sidecar").is_dir() {
                return resolve_dev_dir(&candidate);
            }
        }
    }
    Err(
        "sidecar not found: set ENDLESSLI_SIDECAR_DIR, or create the dev venv with \
         `uv venv --python 3.11 sidecar/.venv && uv pip install -r sidecar/requirements.txt`"
            .to_string(),
    )
}

fn resolve_dev_dir(dir: &Path) -> Result<LaunchPlan, String> {
    #[cfg(windows)]
    let interpreter = dir.join(".venv").join("Scripts").join("python.exe");
    #[cfg(not(windows))]
    let interpreter = dir.join(".venv").join("bin").join("python");
    if interpreter.is_file() {
        return Ok(LaunchPlan::Python {
            interpreter,
            workdir: dir.to_path_buf(),
        });
    }
    // No venv interpreter: fall back to PATH and let the spawn error (which
    // names the missing venv) guide the user if it is unusable.
    #[cfg(windows)]
    let fallback = "python";
    #[cfg(not(windows))]
    let fallback = "python3";
    Ok(LaunchPlan::Python {
        interpreter: PathBuf::from(fallback),
        workdir: dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disables_after_max_restarts() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let manager = SidecarManager::new(tx, PathBuf::from("."));
        assert_eq!(manager.status(), SidecarStatus::Stopped);
        assert_eq!(manager.record_crash(), SidecarStatus::Restarting);
        assert_eq!(manager.record_crash(), SidecarStatus::Restarting);
        assert_eq!(manager.record_crash(), SidecarStatus::Disabled);
    }
}
