//! Shared application state managed by Tauri.
//!
//! Why this exists: command handlers stay thin (implementation plan Section 2)
//! by moving all shared resources here. Phase 0 only tracks directories; later
//! phases add the SQLite pool, sidecar handle, and budget counters.

use std::path::PathBuf;

/// Resources shared across all Tauri commands.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Writable application data directory (SQLite, logs, library files).
    pub data_dir: PathBuf,
    /// Path to the queryable `logs.db` file.
    pub log_db_path: PathBuf,
}
