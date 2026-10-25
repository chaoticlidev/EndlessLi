//! Shared application state managed by Tauri.
//!
//! Command handlers stay thin by moving all shared resources here: the
//! database pool, the sidecar manager, the research run registry, the note
//! embedding queue, and the directories everything lives in.

use std::collections::{HashMap, HashSet};
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
    /// Note ids waiting to be embedded by the background worker.
    pub embed: EmbedQueue,
    /// DuckDB analytical store, when the `analytics` feature is enabled.
    #[cfg(feature = "analytics")]
    pub analytics: Option<Arc<el_analytics::Analytics>>,
    /// Writable application data directory (SQLite, logs, library files).
    pub data_dir: PathBuf,
    /// Path to the queryable `logs.db` file.
    pub log_db_path: PathBuf,
}

impl AppState {
    pub fn kill_sidecar(&self) {
        self.sidecar.kill();
    }
}

/// Note ids awaiting embedding, plus the wake signal the worker waits on.
///
/// A `HashSet` de-duplicates: saving the same note ten times queues it once.
/// The worker drains it after a short debounce (see `embedding::run_worker`).
#[derive(Debug, Clone, Default)]
pub struct EmbedQueue {
    pending: Arc<Mutex<HashSet<String>>>,
    notify: Arc<tokio::sync::Notify>,
}

impl EmbedQueue {
    /// Queues a note id and wakes the worker.
    pub fn enqueue(&self, note_id: impl Into<String>) {
        self.pending
            .lock()
            .expect("embed queue lock")
            .insert(note_id.into());
        self.notify.notify_one();
    }

    /// Queues many note ids at once (re-embed after a model change).
    pub fn enqueue_all(&self, note_ids: impl IntoIterator<Item = String>) {
        {
            let mut pending = self.pending.lock().expect("embed queue lock");
            pending.extend(note_ids);
        }
        self.notify.notify_one();
    }

    /// Resolves once at least one note is queued.
    pub async fn notified(&self) {
        self.notify.notified().await;
    }

    /// Removes and returns everything queued right now.
    pub fn drain(&self) -> Vec<String> {
        let mut pending = self.pending.lock().expect("embed queue lock");
        pending.drain().collect()
    }
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
