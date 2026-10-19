//! Memory repository: user-controlled entries plus the approval queue.
//!
//! AI suggestions land in `memory_suggestions` and only become entries
//! through [`accept_suggestion`]. Nothing here writes memories unprompted.

use el_core::{
    AppError, AppResult, MemoryEntry, MemorySuggestion, NewMemoryEntry, MEMORY_SOURCE_VALUES,
    MEMORY_TYPE_VALUES,
};
use rusqlite::{Connection, Row};

use super::{check_enum, check_not_blank, get_bool, map_db, new_id, now_ts};

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<MemoryEntry> {
    Ok(MemoryEntry {
        id: row.get(0)?,
        memory_type: row.get(1)?,
        project_id: row.get(2)?,
        content: row.get(3)?,
        source: row.get(4)?,
        created_at: row.get(5)?,
        expires_at: row.get(6)?,
        is_active: get_bool(row, 7)?,
    })
}

fn row_to_suggestion(row: &Row<'_>) -> rusqlite::Result<MemorySuggestion> {
    Ok(MemorySuggestion {
        id: row.get(0)?,
        project_id: row.get(1)?,
        content: row.get(2)?,
        session_id: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Creates a memory entry. `project` entries require a project id; `global`
/// entries must not have one (mirrors the SQL `CHECK` with a clear message).
pub fn create_entry(conn: &Connection, input: &NewMemoryEntry) -> AppResult<MemoryEntry> {
    check_enum(&input.memory_type, MEMORY_TYPE_VALUES, "type")?;
    check_enum(&input.source, MEMORY_SOURCE_VALUES, "source")?;
    let content = check_not_blank(&input.content, "memory content")?;
    let is_project = input.memory_type == "project";
    if is_project != input.project_id.is_some() {
        return Err(AppError::InvalidInput(
            "project memories require project_id; global memories must not have one".to_string(),
        ));
    }
    let entry = MemoryEntry {
        id: new_id(),
        memory_type: input.memory_type.clone(),
        project_id: input.project_id.clone(),
        content,
        source: input.source.clone(),
        created_at: now_ts(),
        expires_at: input.expires_at,
        is_active: true,
    };
    conn.execute(
        "INSERT INTO memory_entries(id, type, project_id, content, source, created_at, expires_at, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
        rusqlite::params![
            entry.id,
            entry.memory_type,
            entry.project_id,
            entry.content,
            entry.source,
            entry.created_at,
            entry.expires_at
        ],
    )
    .map_err(map_db)?;
    Ok(entry)
}

/// Lists active global memories, oldest-first.
pub fn list_global(conn: &Connection) -> AppResult<Vec<MemoryEntry>> {
    list_scope(conn, "type = 'global'", None)
}

/// Lists inactive (forgotten) memories, newest-first.
pub fn list_forgotten(conn: &Connection) -> AppResult<Vec<MemoryEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, type, project_id, content, source, created_at, expires_at, is_active
             FROM memory_entries WHERE is_active = 0 ORDER BY created_at DESC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([], row_to_entry).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Lists active memories for one project, oldest-first.
pub fn list_for_project(conn: &Connection, project_id: &str) -> AppResult<Vec<MemoryEntry>> {
    list_scope(
        conn,
        "type = 'project' AND project_id = ?1",
        Some(project_id),
    )
}

fn list_scope(
    conn: &Connection,
    filter: &str,
    project_id: Option<&str>,
) -> AppResult<Vec<MemoryEntry>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, type, project_id, content, source, created_at, expires_at, is_active
             FROM memory_entries WHERE is_active = 1 AND {filter} ORDER BY created_at ASC"
        ))
        .map_err(map_db)?;
    let rows = match project_id {
        Some(id) => stmt.query_map([id], row_to_entry),
        None => stmt.query_map([], row_to_entry),
    }
    .map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Updates an entry's content.
pub fn update_content(conn: &Connection, id: &str, content: &str) -> AppResult<MemoryEntry> {
    let content = check_not_blank(content, "memory content")?;
    let affected = conn
        .execute(
            "UPDATE memory_entries SET content = ?1 WHERE id = ?2",
            (&content, id),
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("memory {id}")));
    }
    get_entry(conn, id)?.ok_or_else(|| AppError::NotFound(format!("memory {id}")))
}

/// Fetches an entry by id, or `None` when it does not exist.
pub fn get_entry(conn: &Connection, id: &str) -> AppResult<Option<MemoryEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, type, project_id, content, source, created_at, expires_at, is_active
             FROM memory_entries WHERE id = ?1",
        )
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_entry(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Restores a soft-deleted entry.
pub fn restore(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute(
            "UPDATE memory_entries SET is_active = 1 WHERE id = ?1",
            [id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("memory {id}")));
    }
    Ok(())
}

/// Soft-deletes an entry (`/forget`). The row stays for audit and restore.
pub fn forget(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute(
            "UPDATE memory_entries SET is_active = 0 WHERE id = ?1",
            [id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("memory {id}")));
    }
    Ok(())
}

/// Permanently removes an entry row.
pub fn delete_permanent(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM memory_entries WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("memory {id}")));
    }
    Ok(())
}

/// Deactivates every entry past its `expires_at`. Returns the count swept.
pub fn sweep_expired(conn: &Connection) -> AppResult<usize> {
    let count = conn
        .execute(
            "UPDATE memory_entries SET is_active = 0
             WHERE is_active = 1 AND expires_at IS NOT NULL AND expires_at <= ?1",
            [now_ts()],
        )
        .map_err(map_db)?;
    Ok(count)
}

/// Queues an AI-suggested memory for user approval.
pub fn suggest(
    conn: &Connection,
    project_id: Option<&str>,
    content: &str,
    session_id: Option<&str>,
) -> AppResult<MemorySuggestion> {
    let content = check_not_blank(content, "suggestion content")?;
    let suggestion = MemorySuggestion {
        id: new_id(),
        project_id: project_id.map(str::to_string),
        content,
        session_id: session_id.map(str::to_string),
        created_at: now_ts(),
    };
    conn.execute(
        "INSERT INTO memory_suggestions(id, project_id, content, session_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            suggestion.id,
            suggestion.project_id,
            suggestion.content,
            suggestion.session_id,
            suggestion.created_at
        ],
    )
    .map_err(map_db)?;
    Ok(suggestion)
}

/// Lists pending suggestions, newest-first.
pub fn list_suggestions(conn: &Connection) -> AppResult<Vec<MemorySuggestion>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, content, session_id, created_at
             FROM memory_suggestions ORDER BY created_at DESC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([], row_to_suggestion).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Approves a suggestion: it becomes a memory entry and leaves the queue,
/// atomically. Project-scoped suggestions become `project` entries.
pub fn accept_suggestion(conn: &mut Connection, id: &str) -> AppResult<MemoryEntry> {
    let tx = conn
        .transaction()
        .map_err(|err| AppError::Database(err.to_string()))?;
    let suggestion: MemorySuggestion = tx
        .query_row(
            "SELECT id, project_id, content, session_id, created_at FROM memory_suggestions WHERE id = ?1",
            [id],
            row_to_suggestion,
        )
        .map_err(|err| {
            if err == rusqlite::Error::QueryReturnedNoRows {
                AppError::NotFound(format!("suggestion {id}"))
            } else {
                map_db(err)
            }
        })?;
    let memory_type = if suggestion.project_id.is_some() {
        "project"
    } else {
        "global"
    };
    let entry = MemoryEntry {
        id: new_id(),
        memory_type: memory_type.to_string(),
        project_id: suggestion.project_id.clone(),
        content: suggestion.content.clone(),
        source: "ai_suggested".to_string(),
        created_at: now_ts(),
        expires_at: None,
        is_active: true,
    };
    tx.execute(
        "INSERT INTO memory_entries(id, type, project_id, content, source, created_at, expires_at, is_active)
         VALUES (?1, ?2, ?3, ?4, 'ai_suggested', ?5, NULL, 1)",
        rusqlite::params![
            entry.id,
            entry.memory_type,
            entry.project_id,
            entry.content,
            entry.created_at
        ],
    )
    .map_err(map_db)?;
    tx.execute("DELETE FROM memory_suggestions WHERE id = ?1", [id])
        .map_err(map_db)?;
    tx.commit()
        .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(entry)
}

/// Discards a suggestion without creating a memory.
pub fn reject_suggestion(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM memory_suggestions WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("suggestion {id}")));
    }
    Ok(())
}

/// Toggles the global memory kill-switch backing the Settings UI.
pub fn set_enabled(conn: &Connection, enabled: bool) -> AppResult<()> {
    super::cache::set_setting(
        conn,
        "memory.enabled",
        if enabled { "true" } else { "false" },
    )
}

/// Reports the kill-switch state. Memory is enabled unless explicitly off.
pub fn is_enabled(conn: &Connection) -> AppResult<bool> {
    match super::cache::get_setting(conn, "memory.enabled")? {
        Some(value) => Ok(value != "false"),
        None => Ok(true),
    }
}
