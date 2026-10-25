//! Python sidecar lifecycle and JSON-RPC client (line-delimited stdio).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use el_core::{AppError, AppResult};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

pub const MAX_RESTARTS: u32 = 1000;
pub const RESTART_WINDOW_SECS: u64 = 300;
pub const CALL_TIMEOUT_SECS: u64 = 90;
pub const HEALTH_TIMEOUT_SECS: u64 = 60;
/// Delay before restarting a crashed sidecar to let the OS reclaim the old process.
const CRASH_BACKOFF_MS: u64 = 2000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SidecarStatus {
    #[default]
    Stopped,
    Running,
    Restarting,
    Disabled,
}

#[derive(Debug)]
pub struct SidecarRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone)]
enum LaunchPlan {
    Binary,
    Python {
        interpreter: PathBuf,
        workdir: PathBuf,
    },
}

/// Holds either a Tauri sidecar handle or a Tokio child, depending on mode.
enum ActiveChild {
    Tauri(CommandChild),
    Tokio(tokio::process::Child),
}

impl std::fmt::Debug for ActiveChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActiveChild::Tauri(_) => write!(f, "ActiveChild::Tauri"),
            ActiveChild::Tokio(_) => write!(f, "ActiveChild::Tokio"),
        }
    }
}

/// A unified wrapper for writing to the sidecar's stdin (dev mode only).
enum ChildStdin {
    Tokio(tokio::process::ChildStdin),
}

impl std::fmt::Debug for ChildStdin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChildStdin::Tokio(_) => write!(f, "ChildStdin::Tokio"),
        }
    }
}

impl ChildStdin {
    async fn write_line(&mut self, line: &str) -> Result<(), String> {
        let payload = format!("{line}\n");
        match self {
            ChildStdin::Tokio(stdin) => {
                stdin
                    .write_all(payload.as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
                stdin.flush().await.map_err(|e| e.to_string())
            }
        }
    }
}

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
    child: Mutex<Option<ActiveChild>>,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, SidecarRpcError>>>>,
    status_tx: mpsc::UnboundedSender<SidecarStatus>,
    model_cache_dir: PathBuf,
    app_handle: OnceLock<tauri::AppHandle>,
}

impl SidecarManager {
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
                app_handle: OnceLock::new(),
            }),
        }
    }

    pub fn set_app_handle(&self, app: tauri::AppHandle) {
        let _ = self.inner.app_handle.set(app);
    }

    pub fn status(&self) -> SidecarStatus {
        *self.inner.status.lock().expect("sidecar status lock")
    }

    pub fn kill(&self) {
        let mut child_guard = self.inner.child.lock().expect("child lock");
        if let Some(child) = child_guard.take() {
            match child {
                ActiveChild::Tauri(child) => {
                    let _ = child.kill();
                }
                ActiveChild::Tokio(mut child) => {
                    let _ = child.kill();
                }
            }
        }
        let _ = self.inner.stdin.try_lock().map(|mut s| *s = None);
        self.inner.pending.lock().expect("pending lock").clear();
        self.set_status(SidecarStatus::Stopped);
    }

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

    pub async fn call(&self, method: &str, params: Value) -> AppResult<Value> {
        self.ensure_running().await?;
        self.raw_call(method, params, CALL_TIMEOUT_SECS).await
    }

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
            if let Some(stdin) = stdin_guard.as_mut() {
                let payload = serde_json::to_string(&line)
                    .map_err(|err| AppError::Internal(format!("encode sidecar request: {err}")))?;
                stdin
                    .write_line(&payload)
                    .await
                    .map_err(|err| AppError::Internal(format!("write to sidecar: {err}")))
            } else {
                let mut child_guard = self.inner.child.lock().expect("child lock");
                match child_guard.as_mut() {
                    Some(ActiveChild::Tauri(child)) => {
                        let payload = serde_json::to_string(&line).map_err(|err| {
                            AppError::Internal(format!("encode sidecar request: {err}"))
                        })?;
                        child
                            .write(format!("{payload}\n").as_bytes())
                            .map_err(|err| AppError::Internal(format!("write to sidecar: {err}")))
                    }
                    _ => Err(AppError::Internal("no active sidecar handle".to_string())),
                }
            }
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

    async fn ensure_running(&self) -> AppResult<()> {
        if self.is_alive() {
            return Ok(());
        }
        self.spawn().await
    }

    async fn spawn(&self) -> AppResult<()> {
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

        // Give the OS time to reap the crashed process and free the port/stdin
        // handle before we spawn a replacement.
        tokio::time::sleep(std::time::Duration::from_millis(CRASH_BACKOFF_MS)).await;

        let plan = resolve_launch().map_err(AppError::Internal)?;

        match &plan {
            LaunchPlan::Binary => self.spawn_tauri().await,
            LaunchPlan::Python {
                interpreter,
                workdir,
            } => self.spawn_tokio(interpreter, workdir).await,
        }
    }

    fn is_alive(&self) -> bool {
        let mut child_guard = self.inner.child.lock().expect("child lock");
        match child_guard.as_mut() {
            Some(ActiveChild::Tauri(_)) => true,
            Some(ActiveChild::Tokio(child)) => {
                child.try_wait().map(|exit| exit.is_none()).unwrap_or(false)
            }
            None => false,
        }
    }

    fn set_status(&self, status: SidecarStatus) {
        *self.inner.status.lock().expect("sidecar status lock") = status;
        let _ = self.inner.status_tx.send(status);
    }

    async fn spawn_tauri(&self) -> AppResult<()> {
        let app = self
            .inner
            .app_handle
            .get()
            .ok_or_else(|| AppError::Internal("AppHandle not initialized".to_string()))?;

        let mut command = app
            .shell()
            .sidecar("endlessli-sidecar")
            .map_err(|e| AppError::Internal(format!("failed to init sidecar: {e}")))?;

        command = command
            .env("ENDLESSLI_EMBED_CACHE_DIR", &self.inner.model_cache_dir)
            .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1");

        let (mut rx, child) = command
            .spawn()
            .map_err(|err| AppError::Internal(format!("spawn bundled sidecar: {err}")))?;

        *self.inner.child.lock().expect("child lock") = Some(ActiveChild::Tauri(child));
        *self.inner.stdin.lock().await = None;

        self.set_status(SidecarStatus::Running);

        let pending_owner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut buffer = Vec::new();
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(bytes) => {
                        buffer.extend_from_slice(&bytes);

                        // Process all complete lines in the buffer
                        while let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes: Vec<u8> = buffer.drain(..=newline_pos).collect();
                            let line = String::from_utf8_lossy(&line_bytes);
                            handle_stdout_line(&pending_owner, &line);
                        }
                    }
                    CommandEvent::Stderr(line_bytes) => {
                        let line = String::from_utf8_lossy(&line_bytes);
                        if !line.trim().is_empty() {
                            tracing::warn!(target: "sidecar", "{}", line.trim());
                        }
                    }
                    CommandEvent::Terminated(payload) => {
                        tracing::warn!("sidecar terminated: {:?}", payload.code);
                        *pending_owner.child.lock().expect("child lock") = None;
                    }
                    _ => {}
                }
            }
        });

        self.check_health().await
    }

    async fn spawn_tokio(&self, interpreter: &Path, workdir: &Path) -> AppResult<()> {
        let mut command = Command::new(interpreter);
        command
            .arg("-u")
            .arg("-m")
            .arg("endlessli_sidecar")
            .current_dir(workdir)
            .env("PYTHONPATH", workdir)
            .env("PYTHONUNBUFFERED", "1")
            .env("ENDLESSLI_EMBED_CACHE_DIR", &self.inner.model_cache_dir)
            .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|err| AppError::Internal(format!("spawn dev sidecar: {err}")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Internal("no stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Internal("no stderr".to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Internal("no stdin".to_string()))?;

        *self.inner.child.lock().expect("child lock") = Some(ActiveChild::Tokio(child));
        *self.inner.stdin.lock().await = Some(ChildStdin::Tokio(stdin));

        self.set_status(SidecarStatus::Running);

        // --- Use BufReader for Tokio, not `rx` ---
        let pending_owner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                handle_stdout_line(&pending_owner, &line);
            }
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    tracing::warn!(target: "sidecar", "{}", line);
                }
            }
        });

        self.check_health().await
    }

    async fn check_health(&self) -> AppResult<()> {
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
}

fn handle_stdout_line(pending_owner: &Arc<ManagerInner>, line: &str) {
    if line.trim().is_empty() {
        return;
    }
    let response: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!("sidecar sent invalid JSON: {err}");
            return;
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
            let _ = sender.send(Ok(response.get("result").cloned().unwrap_or(Value::Null)));
        }
    }
}

fn resolve_launch() -> Result<LaunchPlan, String> {
    #[cfg(debug_assertions)]
    if let Ok(dir) = std::env::var("ENDLESSLI_SIDECAR_DIR") {
        return resolve_dev_dir(Path::new(&dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["endlessli-sidecar.exe", "endlessli-sidecar"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Ok(LaunchPlan::Binary);
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
    Err("sidecar not found".to_string())
}

fn resolve_dev_dir(dir: &Path) -> Result<LaunchPlan, String> {
    if let Some(interpreter) = dev_interpreter(dir) {
        return Ok(LaunchPlan::Python {
            interpreter,
            workdir: dir.to_path_buf(),
        });
    }
    #[cfg(windows)]
    let fallback = "python";
    #[cfg(not(windows))]
    let fallback = "python3";
    Ok(LaunchPlan::Python {
        interpreter: PathBuf::from(fallback),
        workdir: dir.to_path_buf(),
    })
}

fn dev_interpreter(dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    let interpreter = dir.join(".venv").join("Scripts").join("python.exe");
    #[cfg(not(windows))]
    let interpreter = dir.join(".venv").join("bin").join("python");
    interpreter.is_file().then_some(interpreter)
}

pub fn script_interpreter() -> PathBuf {
    if let Ok(dir) = std::env::var("ENDLESSLI_SIDECAR_DIR") {
        if let Some(interpreter) = dev_interpreter(Path::new(&dir)) {
            return interpreter;
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for candidate in [cwd.join("sidecar"), cwd.join("../sidecar")] {
            if let Some(interpreter) = dev_interpreter(&candidate) {
                return interpreter;
            }
        }
    }
    #[cfg(windows)]
    {
        PathBuf::from("python")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("python3")
    }
}

pub async fn probe_interpreter(path: &Path) -> bool {
    let mut command = Command::new(path);
    command
        .arg("-I")
        .arg("-c")
        .arg("import sys")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    match command.spawn() {
        Ok(mut child) => {
            matches!(
                tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await,
                Ok(Ok(status)) if status.success()
            )
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disables_after_max_restarts() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let manager = SidecarManager::new(tx, PathBuf::from("."));
        assert_eq!(manager.status(), SidecarStatus::Stopped);
        for _ in 0..MAX_RESTARTS {
            assert_eq!(manager.record_crash(), SidecarStatus::Restarting);
        }
        assert_eq!(manager.record_crash(), SidecarStatus::Disabled);
    }
}
