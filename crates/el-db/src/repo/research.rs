//! Research repository: sessions, steps, results, and token accounting.
//!
//! Step and result ordering is assigned by the repository (`MAX + 1`), so
//! callers never coordinate sequence numbers.

use el_core::{
    AppError, AppResult, NewResult, NewSession, NewStep, ResearchResult, ResearchSession,
    ResearchStep, RESEARCH_MODE_VALUES, RESEARCH_STATUS_VALUES, RESULT_TYPE_VALUES,
    STEP_STATUS_VALUES,
};
use rusqlite::{Connection, Row};

use super::notes::row_to_note;
use super::{bool_param, check_enum, check_not_blank, get_bool, map_db, new_id, now_ts};

fn row_to_session(row: &Row<'_>) -> rusqlite::Result<ResearchSession> {
    Ok(ResearchSession {
        id: row.get(0)?,
        query: row.get(1)?,
        mode: row.get(2)?,
        status: row.get(3)?,
        project_id: row.get(4)?,
        folder_id: row.get(5)?,
        context_memory: row.get(6)?,
        tokens_in: row.get(7)?,
        tokens_out: row.get(8)?,
        created_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

fn row_to_step(row: &Row<'_>) -> rusqlite::Result<ResearchStep> {
    Ok(ResearchStep {
        id: row.get(0)?,
        session_id: row.get(1)?,
        parent_step_id: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        step_order: row.get(5)?,
        details: row.get(6)?,
    })
}

fn row_to_result(row: &Row<'_>) -> rusqlite::Result<ResearchResult> {
    Ok(ResearchResult {
        id: row.get(0)?,
        session_id: row.get(1)?,
        result_type: row.get(2)?,
        content: row.get(3)?,
        source_url: row.get(4)?,
        source_title: row.get(5)?,
        result_order: row.get(6)?,
        selected_for_note: get_bool(row, 7)?,
    })
}

/// Creates a session in `pending` status.
pub fn create_session(conn: &Connection, input: &NewSession) -> AppResult<ResearchSession> {
    let query = check_not_blank(&input.query, "query")?;
    check_enum(&input.mode, RESEARCH_MODE_VALUES, "mode")?;
    let session = ResearchSession {
        id: new_id(),
        query,
        mode: input.mode.clone(),
        status: "pending".to_string(),
        project_id: input.project_id.clone(),
        folder_id: input.folder_id.clone(),
        context_memory: input.context_memory.clone(),
        tokens_in: 0,
        tokens_out: 0,
        created_at: now_ts(),
        completed_at: None,
    };
    conn.execute(
        "INSERT INTO research_sessions(id, query, mode, status, project_id, folder_id, context_memory, tokens_in, tokens_out, created_at, completed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, ?8, NULL)",
        rusqlite::params![
            session.id,
            session.query,
            session.mode,
            session.status,
            session.project_id,
            session.folder_id,
            session.context_memory,
            session.created_at
        ],
    )
    .map_err(map_db)?;
    Ok(session)
}

/// Fetches a session by id, or `None` when it does not exist.
pub fn get_session(conn: &Connection, id: &str) -> AppResult<Option<ResearchSession>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, query, mode, status, project_id, folder_id, context_memory,
             tokens_in, tokens_out, created_at, completed_at
             FROM research_sessions WHERE id = ?1",
        )
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_session(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Fetches a session by id, or returns `NotFound`.
pub fn require_session(conn: &Connection, id: &str) -> AppResult<ResearchSession> {
    get_session(conn, id)?.ok_or_else(|| AppError::NotFound(format!("session {id}")))
}

/// Lists sessions newest-first, optionally filtered by status.
pub fn list_sessions(
    conn: &Connection,
    status: Option<&str>,
    limit: usize,
) -> AppResult<Vec<ResearchSession>> {
    let mut stmt = if let Some(status) = status {
        check_enum(status, RESEARCH_STATUS_VALUES, "status")?;
        conn.prepare(
            "SELECT id, query, mode, status, project_id, folder_id, context_memory,
             tokens_in, tokens_out, created_at, completed_at
             FROM research_sessions WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2",
        )
        .map_err(map_db)?
    } else {
        conn.prepare(
            "SELECT id, query, mode, status, project_id, folder_id, context_memory,
             tokens_in, tokens_out, created_at, completed_at
             FROM research_sessions ORDER BY created_at DESC LIMIT ?1",
        )
        .map_err(map_db)?
    };
    let rows = if let Some(status) = status {
        stmt.query_map(rusqlite::params![status, limit as i64], row_to_session)
    } else {
        stmt.query_map([limit as i64], row_to_session)
    }
    .map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Moves a session to a new status. Terminal states stamp `completed_at`.
pub fn set_session_status(conn: &Connection, id: &str, status: &str) -> AppResult<ResearchSession> {
    check_enum(status, RESEARCH_STATUS_VALUES, "status")?;
    let mut session = require_session(conn, id)?;
    session.status = status.to_string();
    session.completed_at = match status {
        "completed" | "failed" | "cancelled" => Some(now_ts()),
        _ => None,
    };
    let affected = conn
        .execute(
            "UPDATE research_sessions SET status = ?1, completed_at = ?2 WHERE id = ?3",
            rusqlite::params![session.status, session.completed_at, id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("session {id}")));
    }
    Ok(session)
}

/// Adds a step, assigning the next `step_order` within its parent scope.
pub fn add_step(conn: &Connection, input: &NewStep) -> AppResult<ResearchStep> {
    check_not_blank(&input.description, "step description")?;
    check_enum(&input.status, STEP_STATUS_VALUES, "status")?;
    require_session(conn, &input.session_id)?;
    let next: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(step_order), -1) + 1 FROM research_steps
             WHERE session_id = ?1 AND COALESCE(parent_step_id, '') = COALESCE(?2, '')",
            rusqlite::params![input.session_id, input.parent_step_id],
            |row| row.get(0),
        )
        .map_err(map_db)?;
    let step = ResearchStep {
        id: new_id(),
        session_id: input.session_id.clone(),
        parent_step_id: input.parent_step_id.clone(),
        description: input.description.clone(),
        status: input.status.clone(),
        step_order: next,
        details: input.details.clone(),
    };
    conn.execute(
        "INSERT INTO research_steps(id, session_id, parent_step_id, description, status, step_order, details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            step.id,
            step.session_id,
            step.parent_step_id,
            step.description,
            step.status,
            step.step_order,
            step.details
        ],
    )
    .map_err(map_db)?;
    Ok(step)
}

/// Lists a session's steps in execution order.
pub fn list_steps(conn: &Connection, session_id: &str) -> AppResult<Vec<ResearchStep>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, parent_step_id, description, status, step_order, details
             FROM research_steps WHERE session_id = ?1 ORDER BY step_order ASC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([session_id], row_to_step).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Moves a step to a new status.
pub fn set_step_status(conn: &Connection, id: &str, status: &str) -> AppResult<()> {
    check_enum(status, STEP_STATUS_VALUES, "status")?;
    let affected = conn
        .execute(
            "UPDATE research_steps SET status = ?1 WHERE id = ?2",
            (status, id),
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("step {id}")));
    }
    Ok(())
}

/// Adds a result, assigning the next `result_order` in the session.
pub fn add_result(conn: &Connection, input: &NewResult) -> AppResult<ResearchResult> {
    check_enum(&input.result_type, RESULT_TYPE_VALUES, "type")?;
    require_session(conn, &input.session_id)?;
    let next: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(result_order), -1) + 1 FROM research_results WHERE session_id = ?1",
            [&input.session_id],
            |row| row.get(0),
        )
        .map_err(map_db)?;
    let result = ResearchResult {
        id: new_id(),
        session_id: input.session_id.clone(),
        result_type: input.result_type.clone(),
        content: input.content.clone(),
        source_url: input.source_url.clone(),
        source_title: input.source_title.clone(),
        result_order: next,
        selected_for_note: false,
    };
    conn.execute(
        "INSERT INTO research_results(id, session_id, type, content, source_url, source_title, result_order, selected_for_note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        rusqlite::params![
            result.id,
            result.session_id,
            result.result_type,
            result.content,
            result.source_url,
            result.source_title,
            result.result_order
        ],
    )
    .map_err(map_db)?;
    Ok(result)
}

/// Lists a session's results in order.
pub fn list_results(conn: &Connection, session_id: &str) -> AppResult<Vec<ResearchResult>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, type, content, source_url, source_title, result_order, selected_for_note
             FROM research_results WHERE session_id = ?1 ORDER BY result_order ASC",
        )
        .map_err(map_db)?;
    let rows = stmt
        .query_map([session_id], row_to_result)
        .map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Marks a result selected (or not) for saving as a note.
pub fn set_result_selected(conn: &Connection, id: &str, selected: bool) -> AppResult<()> {
    let affected = conn
        .execute(
            "UPDATE research_results SET selected_for_note = ?1 WHERE id = ?2",
            rusqlite::params![bool_param(selected), id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("result {id}")));
    }
    Ok(())
}

/// Attaches a saved note to a session. Repeat calls are harmless.
pub fn attach_note(conn: &Connection, session_id: &str, note_id: &str) -> AppResult<()> {
    match conn.execute(
        "INSERT INTO session_notes(session_id, note_id) VALUES (?1, ?2)",
        (session_id, note_id),
    ) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(info, _))
            if info.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || info.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            Ok(())
        }
        Err(other) => Err(map_db(other)),
    }
}

/// Lists the notes saved from a session, newest-first.
pub fn notes_for_session(conn: &Connection, session_id: &str) -> AppResult<Vec<el_core::Note>> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, n.content, n.project_id, n.folder_id, n.author,
             n.source_session_id, n.created_from_note_id, n.embedding_model,
             n.embedding_hash, n.created_at, n.updated_at
             FROM notes n JOIN session_notes sn ON sn.note_id = n.id
             WHERE sn.session_id = ?1 ORDER BY n.created_at DESC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([session_id], row_to_note).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Adds token counts to a session and logs the call, atomically.
pub fn add_tokens(
    conn: &mut Connection,
    session_id: &str,
    provider: &str,
    model: &str,
    tokens_in: i64,
    tokens_out: i64,
) -> AppResult<()> {
    if tokens_in < 0 || tokens_out < 0 {
        return Err(AppError::InvalidInput(
            "token counts must not be negative".to_string(),
        ));
    }
    let tx = conn
        .transaction()
        .map_err(|err| AppError::Database(err.to_string()))?;
    let affected = tx
        .execute(
            "UPDATE research_sessions SET tokens_in = tokens_in + ?1, tokens_out = tokens_out + ?2 WHERE id = ?3",
            rusqlite::params![tokens_in, tokens_out, session_id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("session {session_id}")));
    }
    tx.execute(
        "INSERT INTO token_usage(session_id, provider, model, tokens_in, tokens_out, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![session_id, provider, model, tokens_in, tokens_out, now_ts()],
    )
    .map_err(map_db)?;
    tx.commit()
        .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(())
}
