//! Link commands: manual linking, auto-link suggestions and approval, and
//! rejection for the editor links panel.

use el_core::{AppResult, LinkCandidate, NoteLink};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::embedding;
use crate::state::AppState;

/// Fresh auto-linking candidates for a note, emitted as `linking://suggestions`
/// after the note is (re)embedded.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "linking://suggestions")]
pub struct LinkingSuggestionsEvent {
    pub note_id: String,
    pub candidates: Vec<LinkCandidate>,
}

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

/// Returns the current auto-linking candidates for a note: the top matches by
/// cosine similarity above the configured threshold, excluding existing links
/// and past rejections.
#[tauri::command]
#[specta::specta]
pub fn link_suggestions(
    state: State<'_, AppState>,
    note_id: String,
) -> AppResult<Vec<LinkCandidate>> {
    let model = embedding::active_model(&state)?;
    let threshold = embedding::active_threshold(&state)?;
    el_linking::suggest_links(
        &state.db,
        &note_id,
        &model,
        threshold,
        el_linking::MAX_CANDIDATES,
    )
}

/// Approves a suggested link, writing it once (canonical order) with origin
/// `auto_approved`.
#[tauri::command]
#[specta::specta]
pub fn link_approve(
    state: State<'_, AppState>,
    a: String,
    b: String,
    similarity: Option<f64>,
) -> AppResult<NoteLink> {
    let conn = state.db.connect()?;
    el_db::repo::links::approve(&conn, &a, &b, similarity, "auto_approved")
}
