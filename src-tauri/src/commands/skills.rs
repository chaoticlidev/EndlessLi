//! Skills and scripts commands (Phase 10).
//!
//! Skills store a full Markdown document (frontmatter + template); scripts
//! store full Python source (metadata docstring + code). Handlers parse to
//! validate, run through `el-skills`, and keep every key/provider concern in
//! the shared [`crate::commands::provider`] helper.

use el_core::{AppError, AppResult, NewNote, Note, Script, Skill};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// Whether a Python interpreter is present for the Scripts tab.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ScriptRuntime {
    pub available: bool,
    pub interpreter: Option<String>,
}

/// Lists skills alphabetically.
#[tauri::command]
#[specta::specta]
pub fn skill_list(state: State<'_, AppState>) -> AppResult<Vec<Skill>> {
    let conn = state.db.connect()?;
    el_db::repo::skills::list_skills(&conn)
}

/// Fetches one skill by name.
#[tauri::command]
#[specta::specta]
pub fn skill_get(state: State<'_, AppState>, name: String) -> AppResult<Option<Skill>> {
    let conn = state.db.connect()?;
    el_db::repo::skills::get_skill_by_name(&conn, &name)
}

/// Parses and validates a skill document without saving it (editor preview).
#[tauri::command]
#[specta::specta]
pub fn skill_parse(document: String) -> AppResult<el_skills::SkillDoc> {
    el_skills::parse_skill(&document)
}

/// Validates and saves a skill document. When `previous_name` is supplied and
/// differs from the document's name, the existing row is renamed (the editor's
/// frontmatter `name:` is editable).
#[tauri::command]
#[specta::specta]
pub fn skill_save(
    state: State<'_, AppState>,
    document: String,
    previous_name: Option<String>,
) -> AppResult<Skill> {
    let doc = el_skills::parse_skill(&document)?;
    let input = el_skills::skill::to_new_skill(&document, &doc)?;
    let conn = state.db.connect()?;
    match previous_name.filter(|previous| !previous.trim().is_empty()) {
        Some(previous) if previous != input.name => {
            el_db::repo::skills::save_skill_with_rename(&conn, &input, &previous)
        }
        _ => el_db::repo::skills::save_skill(&conn, &input),
    }
}

/// Deletes a skill by name.
#[tauri::command]
#[specta::specta]
pub fn skill_delete(state: State<'_, AppState>, name: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::skills::delete_skill(&conn, &name)
}

/// Renders a skill with the supplied values and completes it through the
/// configured provider.
#[tauri::command]
#[specta::specta]
pub async fn skill_run(
    state: State<'_, AppState>,
    name: String,
    values: Vec<el_skills::ParamValue>,
) -> AppResult<el_skills::SkillRunOutcome> {
    let skill = {
        let conn = state.db.connect()?;
        el_db::repo::skills::get_skill_by_name(&conn, &name)?
            .ok_or_else(|| AppError::NotFound(format!("skill {name}")))?
    };
    let doc = el_skills::parse_skill(&skill.content)?;
    let provider = crate::commands::provider::build_provider(&state)?;
    let model = crate::commands::provider::configured_model(&state)?;
    el_skills::run_skill(&state.db, &provider, &model, &doc, &values).await
}

/// Saves a skill's output as an AI-authored note.
#[tauri::command]
#[specta::specta]
pub fn skill_save_output(
    state: State<'_, AppState>,
    name: String,
    output: String,
    title: Option<String>,
    project_id: Option<String>,
    folder_id: Option<String>,
) -> AppResult<Note> {
    let output = output.trim().to_string();
    if output.is_empty() {
        return Err(AppError::InvalidInput(
            "the skill produced no output to save".to_string(),
        ));
    }
    let title = title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| name.clone());
    let conn = state.db.connect()?;
    let note = el_db::repo::notes::create(
        &conn,
        &NewNote {
            title,
            content: output,
            project_id,
            folder_id,
            author: "ai".to_string(),
            source_session_id: None,
            created_from_note_id: None,
        },
    )?;
    drop(conn);
    state.embed.enqueue(note.id.clone());
    Ok(note)
}

/// Saves a script's output as an AI-authored note.
#[tauri::command]
#[specta::specta]
pub fn script_save_output(
    state: State<'_, AppState>,
    name: String,
    output: String,
    title: Option<String>,
    project_id: Option<String>,
    folder_id: Option<String>,
) -> AppResult<Note> {
    let output = output.trim().to_string();
    if output.is_empty() {
        return Err(AppError::InvalidInput(
            "the script produced no output to save".to_string(),
        ));
    }
    let title = title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| name.clone());
    let conn = state.db.connect()?;
    let note = el_db::repo::notes::create(
        &conn,
        &NewNote {
            title,
            content: output,
            project_id,
            folder_id,
            author: "ai".to_string(),
            source_session_id: None,
            created_from_note_id: None,
        },
    )?;
    drop(conn);
    state.embed.enqueue(note.id.clone());
    Ok(note)
}

/// Lists scripts alphabetically.
#[tauri::command]
#[specta::specta]
pub fn script_list(state: State<'_, AppState>) -> AppResult<Vec<Script>> {
    let conn = state.db.connect()?;
    el_db::repo::skills::list_scripts(&conn)
}

/// Fetches one script by name.
#[tauri::command]
#[specta::specta]
pub fn script_get(state: State<'_, AppState>, name: String) -> AppResult<Option<Script>> {
    let conn = state.db.connect()?;
    el_db::repo::skills::get_script_by_name(&conn, &name)
}

/// Parses and validates a script document without saving it (editor preview).
#[tauri::command]
#[specta::specta]
pub fn script_parse(document: String) -> AppResult<el_skills::ScriptDoc> {
    el_skills::parse_script(&document)
}

/// Validates and saves a script document. When `previous_name` is supplied and
/// differs from the document's name, the existing row is renamed.
#[tauri::command]
#[specta::specta]
pub fn script_save(
    state: State<'_, AppState>,
    document: String,
    previous_name: Option<String>,
) -> AppResult<Script> {
    let doc = el_skills::parse_script(&document)?;
    let input = el_skills::script::to_new_script(&document, &doc)?;
    let conn = state.db.connect()?;
    match previous_name.filter(|previous| !previous.trim().is_empty()) {
        Some(previous) if previous != input.name => {
            el_db::repo::skills::save_script_with_rename(&conn, &input, &previous)
        }
        _ => el_db::repo::skills::save_script(&conn, &input),
    }
}

/// Deletes a script by name.
#[tauri::command]
#[specta::specta]
pub fn script_delete(state: State<'_, AppState>, name: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    el_db::repo::skills::delete_script(&conn, &name)
}

/// Runs a script in the sandbox. `allow_network` defaults to the script's
/// declared value; the timeout is clamped to a sane range.
#[tauri::command]
#[specta::specta]
pub async fn script_run(
    state: State<'_, AppState>,
    name: String,
    values: Vec<el_skills::ParamValue>,
    allow_network: Option<bool>,
    timeout_secs: Option<u32>,
) -> AppResult<el_skills::ScriptOutcome> {
    let script = {
        let conn = state.db.connect()?;
        el_db::repo::skills::get_script_by_name(&conn, &name)?
            .ok_or_else(|| AppError::NotFound(format!("script {name}")))?
    };
    let doc = el_skills::parse_script(&script.content)?;
    let allow_network = allow_network.unwrap_or(doc.allow_network);
    let seconds = timeout_secs
        .unwrap_or(el_skills::DEFAULT_SCRIPT_TIMEOUT_SECS)
        .clamp(1, 600);
    let interpreter = crate::sidecar::script_interpreter();
    el_skills::run_script(
        &interpreter,
        &script.content,
        &values,
        allow_network,
        std::time::Duration::from_secs(u64::from(seconds)),
    )
    .await
}

/// Reports whether a Python interpreter is available for the Scripts tab.
#[tauri::command]
#[specta::specta]
pub async fn script_runtime() -> AppResult<ScriptRuntime> {
    let interpreter = crate::sidecar::script_interpreter();
    let available = crate::sidecar::probe_interpreter(&interpreter).await;
    Ok(ScriptRuntime {
        available,
        interpreter: Some(interpreter.display().to_string()),
    })
}

/// Fills an empty skills/scripts table with the built-ins (task 10.7).
///
/// Idempotent and non-destructive: a table that already has rows is left
/// untouched, so a user who deletes a built-in keeps it deleted.
pub fn seed_builtins(conn: &rusqlite::Connection) -> AppResult<()> {
    if el_db::repo::skills::list_skills(conn)?.is_empty() {
        for skill in el_skills::builtins::builtin_skills()? {
            el_db::repo::skills::save_skill(conn, &skill)?;
        }
    }
    if el_db::repo::skills::list_scripts(conn)?.is_empty() {
        for script in el_skills::builtins::builtin_scripts()? {
            el_db::repo::skills::save_script(conn, &script)?;
        }
    }
    Ok(())
}
