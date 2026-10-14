//! Note commands: CRUD, search, detail view, and title resolution for links.

use el_core::{AppResult, NewNote, Note, NoteLink, Tag, UpdateNote};
use el_db::repo::{links as link_repo, notes as note_repo, tags as tag_repo};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// Which notes to list.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NoteScope {
    /// Notes with no project.
    Inbox,
    /// Every note, regardless of project.
    All,
    /// Notes of one project.
    Project { project_id: String },
    /// Notes of one folder.
    Folder { folder_id: String },
}

/// A linked note as shown in the links panel.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LinkedNote {
    pub id: String,
    pub title: String,
    pub similarity: Option<f64>,
    pub origin: String,
}

/// A note with its tags and linked notes for the editor view.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NoteDetail {
    pub note: Note,
    pub tags: Vec<Tag>,
    pub links: Vec<LinkedNote>,
}

/// Lists notes for a scope, newest-first.
#[tauri::command]
#[specta::specta]
pub fn note_list(state: State<'_, AppState>, scope: NoteScope) -> AppResult<Vec<Note>> {
    let conn = state.db.connect()?;
    match &scope {
        NoteScope::Inbox => note_repo::list_inbox(&conn),
        NoteScope::All => note_repo::list_all(&conn),
        NoteScope::Project { project_id } => note_repo::list_by_project(&conn, project_id),
        NoteScope::Folder { folder_id } => note_repo::list_by_folder(&conn, folder_id),
    }
}

/// Fetches a note by id.
#[tauri::command]
#[specta::specta]
pub fn note_get(state: State<'_, AppState>, id: String) -> AppResult<Option<Note>> {
    let conn = state.db.connect()?;
    note_repo::get(&conn, &id)
}

/// Fetches a note with its tags and linked notes.
#[tauri::command]
#[specta::specta]
pub fn note_detail(state: State<'_, AppState>, id: String) -> AppResult<Option<NoteDetail>> {
    let conn = state.db.connect()?;
    let Some(note) = note_repo::get(&conn, &id)? else {
        return Ok(None);
    };
    let tags = tag_repo::tags_for_note(&conn, &id)?;
    let raw_links: Vec<NoteLink> = link_repo::list_for_note(&conn, &id)?;
    let mut links = Vec::with_capacity(raw_links.len());
    for link in raw_links {
        let other = if link.note_a == id {
            link.note_b.clone()
        } else {
            link.note_a.clone()
        };
        if let Some(target) = note_repo::get(&conn, &other)? {
            links.push(LinkedNote {
                id: target.id,
                title: target.title,
                similarity: link.similarity,
                origin: link.origin,
            });
        }
    }
    Ok(Some(NoteDetail { note, tags, links }))
}

/// Creates a note.
#[tauri::command]
#[specta::specta]
pub fn note_create(state: State<'_, AppState>, input: NewNote) -> AppResult<Note> {
    let conn = state.db.connect()?;
    note_repo::create(&conn, &input)
}

/// Updates a note's title or content. `None` fields are left unchanged.
#[tauri::command]
#[specta::specta]
pub fn note_update(state: State<'_, AppState>, id: String, patch: UpdateNote) -> AppResult<Note> {
    let mut conn = state.db.connect()?;
    note_repo::update(&mut conn, &id, &patch)
}

/// Moves a note to a project and folder, or unplaces it with `None`.
#[tauri::command]
#[specta::specta]
pub fn note_move(
    state: State<'_, AppState>,
    id: String,
    project_id: Option<String>,
    folder_id: Option<String>,
) -> AppResult<Note> {
    let conn = state.db.connect()?;
    note_repo::move_note(&conn, &id, project_id.as_deref(), folder_id.as_deref())
}

/// Deletes a note with its links, tags, and vectors.
#[tauri::command]
#[specta::specta]
pub fn note_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let mut conn = state.db.connect()?;
    note_repo::delete(&mut conn, &id)
}

/// Copies a note (with its tags) into a branched note recording its origin.
#[tauri::command]
#[specta::specta]
pub fn note_branch(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
) -> AppResult<Note> {
    let mut conn = state.db.connect()?;
    note_repo::branch(&mut conn, &id, title.as_deref())
}

/// Full-text search over titles and content.
#[tauri::command]
#[specta::specta]
pub fn note_search(state: State<'_, AppState>, query: String, limit: u32) -> AppResult<Vec<Note>> {
    let conn = state.db.connect()?;
    note_repo::search(&conn, &query, limit as usize)
}

/// Finds notes by title for `[[wikilink]]` resolution: exact matches first,
// then prefix matches, capped at `limit`.
#[tauri::command]
#[specta::specta]
pub fn note_find_by_title(
    state: State<'_, AppState>,
    title: String,
    limit: u32,
) -> AppResult<Vec<Note>> {
    let conn = state.db.connect()?;
    note_repo::find_by_title(&conn, &title, limit as usize)
}
