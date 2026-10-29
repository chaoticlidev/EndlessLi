//! Settings commands: single-key read/write over the settings table.

use el_core::{AppError, AppResult};
use tauri::State;

use crate::state::AppState;

/// Reads one setting value (JSON), or `None` when unset.
#[tauri::command]
#[specta::specta]
pub fn settings_get(state: State<'_, AppState>, key: String) -> AppResult<Option<String>> {
    check_key(&key)?;
    let conn = state.db.connect()?;
    el_db::repo::cache::get_setting(&conn, &key)
}

/// Writes one setting value (JSON), replacing any previous value.
#[tauri::command]
#[specta::specta]
pub fn settings_update(state: State<'_, AppState>, key: String, value: String) -> AppResult<()> {
    check_key(&key)?;
    if key == "llm.model" && value.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "model must not be blank".to_string(),
        ));
    }
    let conn = state.db.connect()?;
    el_db::repo::cache::set_setting(&conn, &key, &value)
}

/// Rejects blank keys and paths outside the known namespaces so settings
/// stay a flat, auditable surface.
fn check_key(key: &str) -> AppResult<()> {
    const NAMESPACES: &[&str] = &[
        "llm.",
        "embedding.",
        "linking.",
        "cache.",
        "budget.",
        "memory.",
        "research.",
        "audio.",
    ];
    if key.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "setting key must not be blank".to_string(),
        ));
    }
    if NAMESPACES.iter().any(|prefix| key.starts_with(prefix)) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "unknown setting namespace in {key:?}"
        )))
    }
}
