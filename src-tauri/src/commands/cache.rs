//! Cache commands: usage stats and manual clearing.

use el_core::{AppError, AppResult, CacheStats, CACHE_TYPE_VALUES};
use tauri::State;

use crate::state::AppState;

/// Reports entry count and total bytes.
#[tauri::command]
#[specta::specta]
pub fn cache_stats(state: State<'_, AppState>) -> AppResult<CacheStats> {
    let conn = state.db.connect()?;
    el_db::repo::cache::stats(&conn)
}

/// Clears the whole cache, or one entry type, and returns the fresh stats.
#[tauri::command]
#[specta::specta]
pub fn cache_clear(
    state: State<'_, AppState>,
    entry_type: Option<String>,
) -> AppResult<CacheStats> {
    let conn = state.db.connect()?;
    match entry_type {
        Some(entry_type) => {
            if !CACHE_TYPE_VALUES.contains(&entry_type.as_str()) {
                return Err(AppError::InvalidInput(format!(
                    "unknown cache type {entry_type:?}"
                )));
            }
            el_db::repo::cache::clear_by_type(&conn, &entry_type)?;
        }
        None => {
            el_db::repo::cache::clear_all(&conn)?;
        }
    }
    el_db::repo::cache::stats(&conn)
}
