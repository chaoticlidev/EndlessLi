//! Shared application state managed by Tauri.
//!
//! Command handlers stay thin by moving all shared resources here: the
//! database pool plus the directories it lives in.

use std::path::PathBuf;

/// Resources shared across all Tauri commands.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Pooled SQLite handle (migrated on open).
    pub db: el_db::Database,
    /// Writable application data directory (SQLite, logs, library files).
    pub data_dir: PathBuf,
    /// Path to the queryable `logs.db` file.
    pub log_db_path: PathBuf,
}
