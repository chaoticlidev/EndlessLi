//! Project commands: thin IPC wrappers over the project repository.

use el_core::{AppError, AppResult, NewProject, Project, UpdateProject};
use tauri::State;

use crate::state::AppState;

/// Encrypts or decrypts every note in a project (Phase 11.2).
///
/// Enabling generates a key in the OS keychain, encrypts the project's notes
/// in one transaction, and deletes their vectors. Disabling decrypts every
/// note, removes the key, and queues the notes for re-embedding.
#[tauri::command]
#[specta::specta]
pub fn project_encryption_set(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> AppResult<Project> {
    let mut conn = state.db.connect()?;
    let project = el_db::repo::projects::require(&conn, &id)?;
    if project.encrypted == enabled {
        return Ok(project);
    }

    let mut reembed = Vec::new();
    if enabled {
        // Store the key first: if the keychain fails, no note is touched.
        el_security::crypto::enable_project(&id)?;
        let tx = conn
            .transaction()
            .map_err(|err| AppError::Database(err.to_string()))?;
        for (note_id, title, content) in el_db::repo::notes::list_raw_by_project(&tx, &id)? {
            let stored_title = if el_security::crypto::is_encrypted(&title) {
                title
            } else {
                el_security::crypto::encrypt_text(&id, &title)?
            };
            let stored_content = if el_security::crypto::is_encrypted(&content) {
                content
            } else {
                el_security::crypto::encrypt_text(&id, &content)?
            };
            el_db::repo::notes::set_raw_fields(&tx, &note_id, &stored_title, &stored_content)?;
            el_db::vec::delete_embedding(&tx, &note_id)?;
        }
        el_db::repo::projects::set_encrypted(&tx, &id, true)?;
        tx.commit()
            .map_err(|err| AppError::Database(err.to_string()))?;
    } else {
        let tx = conn
            .transaction()
            .map_err(|err| AppError::Database(err.to_string()))?;
        for (note_id, title, content) in el_db::repo::notes::list_raw_by_project(&tx, &id)? {
            let plain_title = if el_security::crypto::is_encrypted(&title) {
                el_security::crypto::decrypt_text(&id, &title)?
            } else {
                title
            };
            let plain_content = if el_security::crypto::is_encrypted(&content) {
                el_security::crypto::decrypt_text(&id, &content)?
            } else {
                content
            };
            el_db::repo::notes::set_raw_fields(&tx, &note_id, &plain_title, &plain_content)?;
            el_db::vec::delete_embedding(&tx, &note_id)?;
            reembed.push(note_id);
        }
        el_db::repo::projects::set_encrypted(&tx, &id, false)?;
        tx.commit()
            .map_err(|err| AppError::Database(err.to_string()))?;
        if let Err(err) = el_security::crypto::disable_project(&id) {
            tracing::warn!(project = %id, %err, "could not delete project encryption key");
        }
        for note_id in reembed {
            state.embed.enqueue(note_id);
        }
    }

    let conn = state.db.connect()?;
    el_db::repo::projects::require(&conn, &id)
}

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
/// Encrypted projects decrypt their notes first, otherwise the foreign-key
/// action would leave orphaned ciphertext with no project key.
#[tauri::command]
#[specta::specta]
pub fn project_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let mut conn = state.db.connect()?;
    let project = el_db::repo::projects::require(&conn, &id)?;
    let mut reembed = Vec::new();
    if project.encrypted {
        let tx = conn
            .transaction()
            .map_err(|err| AppError::Database(err.to_string()))?;
        for (note_id, title, content) in el_db::repo::notes::list_raw_by_project(&tx, &id)? {
            let plain_title = if el_security::crypto::is_encrypted(&title) {
                el_security::crypto::decrypt_text(&id, &title)?
            } else {
                title
            };
            let plain_content = if el_security::crypto::is_encrypted(&content) {
                el_security::crypto::decrypt_text(&id, &content)?
            } else {
                content
            };
            el_db::repo::notes::set_raw_fields(&tx, &note_id, &plain_title, &plain_content)?;
            el_db::vec::delete_embedding(&tx, &note_id)?;
            reembed.push(note_id);
        }
        tx.commit()
            .map_err(|err| AppError::Database(err.to_string()))?;
        if let Err(err) = el_security::crypto::disable_project(&id) {
            tracing::warn!(project = %id, %err, "could not delete project encryption key");
        }
    }
    el_db::repo::projects::delete(&conn, &id)?;
    for note_id in reembed {
        state.embed.enqueue(note_id);
    }
    Ok(())
}
