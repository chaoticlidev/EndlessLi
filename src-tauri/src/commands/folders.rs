//! Folder commands: thin IPC wrappers returning a nested tree.

use el_core::{AppResult, Folder, NewFolder};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// One node of the project folder tree.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct FolderNode {
    pub folder: Folder,
    pub children: Vec<FolderNode>,
}

/// Lists a project's folders as a nested tree, oldest-first at each level.
#[tauri::command]
#[specta::specta]
pub fn folder_tree(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<FolderNode>> {
    let conn = state.db.connect()?;
    let flat = el_db::repo::folders::list_by_project(&conn, &project_id)?;
    Ok(build_tree(&flat, None))
}

fn build_tree(folders: &[Folder], parent: Option<&str>) -> Vec<FolderNode> {
    folders
        .iter()
        .filter(|folder| folder.parent_id.as_deref() == parent)
        .map(|folder| FolderNode {
            folder: folder.clone(),
            children: build_tree(folders, Some(&folder.id)),
        })
        .collect()
}

/// Creates a folder inside a project, optionally nested.
#[tauri::command]
#[specta::specta]
pub fn folder_create(state: State<'_, AppState>, input: NewFolder) -> AppResult<Folder> {
    let conn = state.db.connect()?;
    el_db::repo::folders::create(&conn, &input)
}

/// Renames a folder.
#[tauri::command]
#[specta::specta]
pub fn folder_rename(state: State<'_, AppState>, id: String, name: String) -> AppResult<Folder> {
    let conn = state.db.connect()?;
    el_db::repo::folders::rename(&conn, &id, &name)
}

/// Moves a folder under a new parent, or to the project root with `None`.
#[tauri::command]
#[specta::specta]
pub fn folder_move(
    state: State<'_, AppState>,
    id: String,
    parent_id: Option<String>,
) -> AppResult<Folder> {
    let conn = state.db.connect()?;
    el_db::repo::folders::move_folder(&conn, &id, parent_id.as_deref())
}

/// Deletes a folder. Nested folders cascade; member notes keep their content.
#[tauri::command]
#[specta::specta]
pub fn folder_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::folders::delete(&conn, &id)
}
