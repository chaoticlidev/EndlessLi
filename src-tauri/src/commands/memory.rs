//! Memory commands.

use el_core::{AppResult, MemoryEntry, MemorySuggestion};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MemoryContext {
    pub project_id: Option<String>,
    pub session_memory: Option<String>,
    pub enabled: bool,
}

#[tauri::command]
#[specta::specta]
pub fn memory_list(state: State<'_, AppState>, project_id: Option<String>) -> AppResult<Vec<MemoryEntry>> {
    let conn = state.db.connect()?;
    if let Some(pid) = project_id {
        el_db::repo::memory::list_for_project(&conn, &pid)
    } else {
        el_db::repo::memory::list_global(&conn)
    }
}

#[tauri::command]
#[specta::specta]
pub fn memory_create(
    state: State<'_, AppState>,
    memory_type: String,
    project_id: Option<String>,
    content: String,
    expires_at: Option<i32>,
) -> AppResult<MemoryEntry> {
    let conn = state.db.connect()?;
    el_db::repo::memory::create_entry(
        &conn,
        &el_core::NewMemoryEntry {
            memory_type,
            project_id,
            content,
            source: "user".to_string(),
            expires_at: expires_at.map(|v| v as i64),
        },
    )
}

#[tauri::command]
#[specta::specta]
pub fn memory_update(state: State<'_, AppState>, id: String, content: String) -> AppResult<MemoryEntry> {
    let conn = state.db.connect()?;
    el_db::repo::memory::update_content(&conn, &id, &content)
}

#[tauri::command]
#[specta::specta]
pub fn memory_forget(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::memory::forget(&conn, &id)
}

#[tauri::command]
#[specta::specta]
pub fn memory_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::memory::delete_permanent(&conn, &id)
}

#[tauri::command]
#[specta::specta]
pub fn memory_suggestions(state: State<'_, AppState>) -> AppResult<Vec<MemorySuggestion>> {
    let conn = state.db.connect()?;
    el_db::repo::memory::list_suggestions(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn memory_accept_suggestion(state: State<'_, AppState>, id: String) -> AppResult<MemoryEntry> {
    let mut conn = state.db.connect()?;
    el_db::repo::memory::accept_suggestion(&mut conn, &id)
}

#[tauri::command]
#[specta::specta]
pub fn memory_reject_suggestion(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::memory::reject_suggestion(&conn, &id)
}

#[tauri::command]
#[specta::specta]
pub fn memory_set_enabled(state: State<'_, AppState>, enabled: bool) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::memory::set_enabled(&conn, enabled)
}

#[tauri::command]
#[specta::specta]
pub fn memory_is_enabled(state: State<'_, AppState>) -> AppResult<bool> {
    let conn = state.db.connect()?;
    el_db::repo::memory::is_enabled(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn memory_build_context(
    state: State<'_, AppState>,
    project_id: Option<String>,
    session_memory: Option<String>,
) -> AppResult<String> {
    let conn = state.db.connect()?;
    let enabled = el_db::repo::memory::is_enabled(&conn)?;
    el_memory::build_context(&conn, project_id.as_deref(), session_memory.as_deref(), enabled)
}

#[tauri::command]
#[specta::specta]
pub fn memory_sweep_expired(state: State<'_, AppState>) -> AppResult<i32> {
    let conn = state.db.connect()?;
    el_db::repo::memory::sweep_expired(&conn).map(|n| n as i32)
}
