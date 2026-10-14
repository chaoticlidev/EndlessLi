//! Project commands: thin IPC wrappers over the project repository.

use el_core::{AppResult, NewProject, Project, UpdateProject};
use tauri::State;

use crate::state::AppState;

/// Lists projects, oldest-first.
#[tauri::command]
#[specta::specta]
pub fn project_list(state: State<'_, AppState>, include_archived: bool) -> AppResult<Vec<Project>> {
    let conn = state.db.connect()?;
    el_db::repo::projects::list(&conn, include_archived)
}

/// Creates a project.
#[tauri::command]
#[specta::specta]
pub fn project_create(state: State<'_, AppState>, input: NewProject) -> AppResult<Project> {
    let conn = state.db.connect()?;
    el_db::repo::projects::create(&conn, &input)
}

/// Updates a project's mutable fields.
#[tauri::command]
#[specta::specta]
pub fn project_update(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateProject,
) -> AppResult<Project> {
    let conn = state.db.connect()?;
    el_db::repo::projects::update(&conn, &id, &patch)
}

/// Archives or unarchives a project.
#[tauri::command]
#[specta::specta]
pub fn project_archive(state: State<'_, AppState>, id: String, archived: bool) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::projects::set_archived(&conn, &id, archived)
}

/// Deletes a project. Folders cascade; member notes move to no project.
#[tauri::command]
#[specta::specta]
pub fn project_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::projects::delete(&conn, &id)
}
