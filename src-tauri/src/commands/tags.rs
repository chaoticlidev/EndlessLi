//! Tag commands: thin IPC wrappers over the tag repository.

use el_core::{AppResult, Tag};
use tauri::State;

use crate::state::AppState;

/// Lists every tag alphabetically.
#[tauri::command]
#[specta::specta]
pub fn tag_list(state: State<'_, AppState>) -> AppResult<Vec<Tag>> {
    let conn = state.db.connect()?;
    el_db::repo::tags::list(&conn)
}

/// Creates a tag, or returns the existing tag when the name is taken.
#[tauri::command]
#[specta::specta]
pub fn tag_create(
    state: State<'_, AppState>,
    name: String,
    color: Option<String>,
) -> AppResult<Tag> {
    let conn = state.db.connect()?;
    match el_db::repo::tags::create(&conn, &name, color.as_deref()) {
        Ok(tag) => Ok(tag),
        Err(el_core::AppError::Conflict(_)) => el_db::repo::tags::get_or_create(&conn, &name),
        Err(other) => Err(other),
    }
}

/// Attaches a tag to a note. Repeat calls are harmless.
#[tauri::command]
#[specta::specta]
pub fn tag_assign(state: State<'_, AppState>, note_id: String, tag_id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::tags::assign_to_note(&conn, &note_id, &tag_id)
}

/// Detaches a tag from a note.
#[tauri::command]
#[specta::specta]
pub fn tag_unassign(state: State<'_, AppState>, note_id: String, tag_id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::tags::unassign_from_note(&conn, &note_id, &tag_id)
}

/// Deletes a tag everywhere it is used.
#[tauri::command]
#[specta::specta]
pub fn tag_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::tags::delete(&conn, &id)
}
