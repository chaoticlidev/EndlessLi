//! Script sandbox (task 10.5).
//!
//! A script runs in a **separate child process** with the working directory set
//! to a fresh temp dir. Parameters travel as JSON; the script's stdout is the
//! result. A wall-clock timeout kills a runaway script, and a small Python
//! bootstrap denies socket creation unless the caller allows the network.

use std::path::Path;
use std::time::{Duration, Instant};

use el_core::{AppError, AppResult};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::ParamValue;

/// Cap on captured bytes per stream; excess output is drained and discarded so
/// a chatty script cannot exhaust memory or block on a full pipe.
const OUTPUT_CAP: usize = 1_000_000;

/// The bootstrap every script runs under. It installs the network guard (unless
/// allowed), then executes the real script as `__main__`.
const RUNNER: &str = r#"import os
import runpy
import sys

_MESSAGE = (
    "Network access is disabled for this script. "
    "Enable 'Allow network' in the Scripts run panel to permit outbound connections."
)


def _denied(*_args, **_kwargs):
    raise PermissionError(_MESSAGE)


def _install_network_guard():
    import socket

    class _BlockedSocket(socket.socket):
        def __init__(self, *_args, **_kwargs):
            raise PermissionError(_MESSAGE)

    socket.socket = _BlockedSocket
    socket.create_connection = _denied
    socket.getaddrinfo = _denied


def main():
    if os.environ.get("ENDLESSLI_ALLOW_NETWORK") != "1":
        _install_network_guard()
    script = sys.argv[1]
    sys.argv = [script]
    runpy.run_path(script, run_name="__main__")


if __name__ == "__main__":
    main()
"#;

/// Result of one sandboxed run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ScriptOutcome {
    pub stdout: String,
    pub stderr: String,
    /// Process exit code, or `None` when the script was killed on timeout.
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: i64,
}

/// Runs `source` under `interpreter` in an isolated temp dir.
pub async fn run_script(
    interpreter: &Path,
    source: &str,
    values: &[ParamValue],
    allow_network: bool,
    timeout: Duration,
) -> AppResult<ScriptOutcome> {
    // Random (v4) name + exclusive create: a predictable UUIDv7 path let a
    // same-user process race the writes and swap `main.py`.
    let parent = std::env::temp_dir().join("endlessli-scripts");
    let dir = parent.join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&parent)
        .await
        .map_err(|err| spawn_error("create script temp dir", &err))?;
    tokio::fs::create_dir(&dir)
        .await
        .map_err(|err| spawn_error("create script temp dir", &err))?;
    let script_path = dir.join("main.py");
    let params_path = dir.join("params.json");
    let runner_path = dir.join("_runner.py");
    tokio::fs::write(&script_path, source)
        .await
        .map_err(|err| spawn_error("write script", &err))?;
    tokio::fs::write(&params_path, params_json(values))
        .await
        .map_err(|err| spawn_error("write parameters", &err))?;
    tokio::fs::write(&runner_path, RUNNER)
        .await
        .map_err(|err| spawn_error("write sandbox bootstrap", &err))?;

    let started = Instant::now();
    let mut command = Command::new(interpreter);
    command
        .arg("-I")
        .arg("-X")
        .arg("utf8")
        .arg("-B")
        .arg("_runner.py")
        .arg("main.py")
        .current_dir(&dir)
        .env("ENDLESSLI_INPUT", &params_path)
        .env(
            "ENDLESSLI_ALLOW_NETWORK",
            if allow_network { "1" } else { "0" },
        )
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|err| {
        AppError::Internal(format!(
            "start script interpreter ({}): {err}",
            interpreter.display()
        ))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Internal("script stdout was not piped".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Internal("script stderr was not piped".to_string()))?;
    let out_task = tokio::spawn(read_capped(stdout));
    let err_task = tokio::spawn(read_capped(stderr));

    let (timed_out, exit_code) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => (false, status.code()),
        Ok(Err(err)) => {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(AppError::Internal(format!("wait for script: {err}")));
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            (true, None)
        }
    };
    let stdout = out_task.await.unwrap_or_default();
    let stderr = err_task.await.unwrap_or_default();
    let duration_ms = started.elapsed().as_millis() as i64;
    let _ = tokio::fs::remove_dir_all(&dir).await;

    Ok(ScriptOutcome {
        stdout,
        stderr,
        exit_code,
        timed_out,
        duration_ms,
    })
}

/// Serializes parameter values as a JSON object for the script to read.
fn params_json(values: &[ParamValue]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = values
        .iter()
        .map(|value| {
            (
                value.name.trim().to_string(),
                serde_json::Value::String(value.value.clone()),
            )
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::Value::Object(map))
        .unwrap_or_else(|_| "{}".to_string())
}

/// Reads a stream to EOF, keeping at most [`OUTPUT_CAP`] bytes.
async fn read_capped<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> String {
    let mut collected: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if collected.len() < OUTPUT_CAP {
                    let room = OUTPUT_CAP - collected.len();
                    collected.extend_from_slice(&buffer[..room.min(read)]);
                }
            }
        }
    }
    String::from_utf8_lossy(&collected).to_string()
}

fn spawn_error(action: &str, err: &std::io::Error) -> AppError {
    AppError::Internal(format!("{action}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn value(name: &str, value: &str) -> ParamValue {
        ParamValue {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    /// Locates a usable interpreter: the dev venv first, then PATH. Returns
    /// `None` (the caller skips) when neither responds.
    fn interpreter_or_skip() -> Option<PathBuf> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let candidate = if cfg!(windows) {
            manifest.join("../../sidecar/.venv/Scripts/python.exe")
        } else {
            manifest.join("../../sidecar/.venv/bin/python")
        };
        for path in [candidate, PathBuf::from("python"), PathBuf::from("python3")] {
            let available = std::process::Command::new(&path)
                .arg("--version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
            if available {
                return Some(path);
            }
        }
        None
    }

    #[tokio::test]
    async fn runs_and_passes_parameters() {
        let Some(python) = interpreter_or_skip() else {
            eprintln!("skipping: no Python interpreter available");
            return;
        };
        let source = "import json, os\n\
            params = json.load(open(os.environ['ENDLESSLI_INPUT'], encoding='utf-8'))\n\
            print('hello', params['name'])\n";
        let outcome = run_script(
            &python,
            source,
            &[value("name", "Ada")],
            false,
            Duration::from_secs(30),
        )
        .await
        .expect("run");
        assert!(!outcome.timed_out);
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.stdout.contains("hello Ada"));
    }

    #[tokio::test]
    async fn times_out_a_runaway_script() {
        let Some(python) = interpreter_or_skip() else {
            eprintln!("skipping: no Python interpreter available");
            return;
        };
        let outcome = run_script(
            &python,
            "import time\nprint('start')\ntime.sleep(30)\n",
            &[],
            false,
            Duration::from_millis(700),
        )
        .await
        .expect("run");
        assert!(outcome.timed_out);
        assert_eq!(outcome.exit_code, None);
    }

    #[tokio::test]
    async fn network_is_blocked_unless_allowed() {
        let Some(python) = interpreter_or_skip() else {
            eprintln!("skipping: no Python interpreter available");
            return;
        };
        let source = "import socket\nsocket.socket()\nprint('opened')\n";

        let blocked = run_script(&python, source, &[], false, Duration::from_secs(30))
            .await
            .expect("run");
        assert_ne!(blocked.exit_code, Some(0));
        assert!(blocked.stderr.contains("Network access is disabled"));
        assert!(!blocked.stdout.contains("opened"));

        let allowed = run_script(&python, source, &[], true, Duration::from_secs(30))
            .await
            .expect("run");
        assert_eq!(allowed.exit_code, Some(0));
        assert!(allowed.stdout.contains("opened"));
    }

    #[tokio::test]
    async fn builtin_scripts_run_offline() {
        let Some(python) = interpreter_or_skip() else {
            eprintln!("skipping: no Python interpreter available");
            return;
        };
        let scripts = crate::builtins::builtin_scripts().expect("builtins");
        assert_eq!(scripts.len(), 6);
        
        for script in scripts {
            // Skip scripts that require network access (like YouTube Transcript)
            if script.allow_network {
                continue;
            }

            // Provide a superset of parameters so most scripts work
            let outcome = run_script(
                &python,
                &script.content,
                &[
                    value("text", "the quick brown fox jumps over the lazy dog the end"),
                    value("csv", "name,age\nAda,36\nAlan,41\n"),
                    value("csv_content", "name,age\nAda,36\nAlan,41\n"),
                    value("top", "3"),
                    value("top_n", "3"),
                ],
                false,
                Duration::from_secs(30),
            )
            .await
            .expect("run");
            
            assert!(!outcome.timed_out, "{} timed out", script.name);
            assert_eq!(
                outcome.exit_code,
                Some(0),
                "{} failed: {}",
                script.name,
                outcome.stderr
            );
            assert!(
                !outcome.stdout.trim().is_empty(),
                "{} produced no output",
                script.name
            );
        }
    }
}
