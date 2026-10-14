//! Link commands: manual linking for the editor links panel.

use el_core::{AppResult, NoteLink};
use tauri::State;

use crate::state::AppState;

/// Creates a manual link between two notes.
#[tauri::command]
#[specta::specta]
pub fn link_create_manual(state: State<'_, AppState>, a: String, b: String) -> AppResult<NoteLink> {
    let conn = state.db.connect()?;
    el_db::repo::links::create_manual(&conn, &a, &b, None)
}

/// Removes a link between two notes.
#[tauri::command]
#[specta::specta]
pub fn link_remove(state: State<'_, AppState>, a: String, b: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::links::remove(&conn, &a, &b)
}

/// Lists every link touching a note, in either direction.
#[tauri::command]
#[specta::specta]
pub fn links_for_note(state: State<'_, AppState>, note_id: String) -> AppResult<Vec<NoteLink>> {
    let conn = state.db.connect()?;
    el_db::repo::links::list_for_note(&conn, &note_id)
}

/// Rejects a pair so it is never suggested again.
#[tauri::command]
#[specta::specta]
pub fn link_reject(state: State<'_, AppState>, a: String, b: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::links::reject(&conn, &a, &b)
}
