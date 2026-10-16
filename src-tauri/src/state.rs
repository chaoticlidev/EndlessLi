//! Shared application state managed by Tauri.
//!
//! Command handlers stay thin by moving all shared resources here: the
//! database pool, the sidecar manager, the research run registry, and the
//! directories everything lives in.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use crate::sidecar::SidecarManager;

/// Resources shared across all Tauri commands.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Pooled SQLite handle (migrated on open).
    pub db: el_db::Database,
    /// Python sidecar lifecycle and RPC client.
    pub sidecar: SidecarManager,
    /// Cancellation tokens for in-flight research runs, by session id.
    pub research: ResearchRegistry,
    /// Broadcast channel for research progress, forwarded to Tauri events.
    pub research_tx: tokio::sync::mpsc::UnboundedSender<el_research::ResearchEvent>,
    /// Writable application data directory (SQLite, logs, library files).
    pub data_dir: PathBuf,
    /// Path to the queryable `logs.db` file.
    pub log_db_path: PathBuf,
}

/// In-flight research runs. Cancellation is cooperative: the token is set
/// here and checked between orchestration steps.
#[derive(Debug, Clone, Default)]
pub struct ResearchRegistry {
    inner: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl ResearchRegistry {
    /// Tracks a new run.
    pub fn insert(&self, session_id: String, cancel: CancellationToken) {
        self.inner
            .lock()
            .expect("registry lock")
            .insert(session_id, cancel);
    }

    /// Signals cancellation. Idempotent: unknown ids are already finished.
    pub fn cancel(&self, session_id: &str) -> bool {
        self.inner
            .lock()
            .expect("registry lock")
            .get(session_id)
            .map(|token| {
                token.cancel();
                true
            })
            .unwrap_or(false)
    }

    /// Reports whether a run is currently tracked (started and not finished).
    pub fn contains(&self, session_id: &str) -> bool {
        self.inner
            .lock()
            .expect("registry lock")
            .contains_key(session_id)
    }

    /// Forgets a finished run.
    pub fn remove(&self, session_id: &str) {
        self.inner.lock().expect("registry lock").remove(session_id);
    }
}
