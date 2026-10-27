//! Synthesis repository: persisted synthesis jobs and their lifecycle.
//!
//! Mirrors the research session pattern: jobs are created `pending`,
//! updated through `in_progress`, and finalized as `completed`, `failed`,
//! or `cancelled`. Output (the generated media) and token counts are stored
//! so the history panel can replay a finished run.

use el_core::{AppError, AppResult};
use rusqlite::{Connection, Row};

use super::{check_enum, check_not_blank, map_db, new_id, now_ts};

/// Allowed values for `synthesis_jobs.status`. Mirrors the SQL CHECK.
pub const SYNTHESIS_STATUS_VALUES: &[&str] =
    &["pending", "in_progress", "completed", "failed", "cancelled"];

/// Allowed values for `synthesis_jobs.media_type`.
pub const SYNTHESIS_MEDIA_VALUES: &[&str] =
    &["report", "slides", "flashcards", "quiz", "audio", "dataset"];

/// One persisted synthesis job (the DB row mapped to a struct).
#[derive(Debug, Clone)]
pub struct SynthesisJob {
    pub id: String,
    pub title: String,
    pub status: String,
    pub k: i64,
    pub media_type: String,
    pub instructions: Option<String>,
    pub doc_refs: String,
    pub model: Option<String>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub output: Option<String>,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

fn row_to_job(row: &Row<'_>) -> rusqlite::Result<SynthesisJob> {
    Ok(SynthesisJob {
        id: row.get(0)?,
        title: row.get(1)?,
        status: row.get(2)?,
        k: row.get(3)?,
        media_type: row.get(4)?,
        instructions: row.get(5)?,
        doc_refs: row.get(6)?,
        model: row.get(7)?,
        tokens_in: row.get(8)?,
        tokens_out: row.get(9)?,
        output: row.get(10)?,
        error_message: row.get(11)?,
        created_at: row.get(12)?,
        completed_at: row.get(13)?,
    })
}

/// Creates a synthesis job in `pending` status.
pub fn create_job(
    conn: &Connection,
    title: &str,
    k: i64,
    media_type: &str,
    instructions: Option<&str>,
    doc_refs: &str,
) -> AppResult<SynthesisJob> {
    let title = check_not_blank(title, "title")?;
    check_enum(media_type, SYNTHESIS_MEDIA_VALUES, "media_type")?;
    let job = SynthesisJob {
        id: new_id(),
        title,
        status: "pending".to_string(),
        k,
        media_type: media_type.to_string(),
        instructions: instructions
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        doc_refs: doc_refs.to_string(),
        model: None,
        tokens_in: 0,
        tokens_out: 0,
        output: None,
        error_message: None,
        created_at: now_ts(),
        completed_at: None,
    };
    conn.execute(
        "INSERT INTO synthesis_jobs(id, title, status, k, media_type, instructions,
         doc_refs, model, tokens_in, tokens_out, output, error_message,
         created_at, completed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, NULL, NULL, ?9, NULL)",
        rusqlite::params![
            job.id,
            job.title,
            job.status,
            job.k,
            job.media_type,
            job.instructions,
            job.doc_refs,
            job.model,
            job.created_at,
        ],
    )
    .map_err(map_db)?;
    Ok(job)
}

/// Fetches a job by id, or `None` when it does not exist.
pub fn get_job(conn: &Connection, id: &str) -> AppResult<Option<SynthesisJob>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, status, k, media_type, instructions, doc_refs,
             model, tokens_in, tokens_out, output, error_message,
             created_at, completed_at
             FROM synthesis_jobs WHERE id = ?1",
        )
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_job(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Fetches a job by id, or returns `NotFound`.
pub fn require_job(conn: &Connection, id: &str) -> AppResult<SynthesisJob> {
    get_job(conn, id)?.ok_or_else(|| AppError::NotFound(format!("synthesis job {id}")))
}

/// Lists all jobs newest-first.
pub fn list_jobs(conn: &Connection, limit: usize) -> AppResult<Vec<SynthesisJob>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, status, k, media_type, instructions, doc_refs,
             model, tokens_in, tokens_out, output, error_message,
             created_at, completed_at
             FROM synthesis_jobs ORDER BY created_at DESC LIMIT ?1",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([limit as i64], row_to_job).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Moves a job to a new status. Terminal states stamp `completed_at`.
pub fn set_status(conn: &Connection, id: &str, status: &str) -> AppResult<()> {
    check_enum(status, SYNTHESIS_STATUS_VALUES, "status")?;
    let completed_at = match status {
        "completed" | "failed" | "cancelled" => Some(now_ts()),
        _ => None,
    };
    let affected = conn
        .execute(
            "UPDATE synthesis_jobs SET status = ?1, completed_at = ?2 WHERE id = ?3",
            rusqlite::params![status, completed_at, id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("synthesis job {id}")));
    }
    Ok(())
}

/// Stores the generated output and token counts, marking the job completed.
pub fn set_output(
    conn: &Connection,
    id: &str,
    output: &str,
    model: &str,
    tokens_in: i64,
    tokens_out: i64,
) -> AppResult<()> {
    let ts = now_ts();
    let affected = conn
        .execute(
            "UPDATE synthesis_jobs
             SET output = ?1, model = ?2, tokens_in = tokens_in + ?3,
                 tokens_out = tokens_out + ?4, status = 'completed',
                 completed_at = ?5
             WHERE id = ?6",
            rusqlite::params![output, model, tokens_in, tokens_out, ts, id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("synthesis job {id}")));
    }
    Ok(())
}

/// Stores an error message and marks the job failed.
pub fn set_error(conn: &Connection, id: &str, message: &str) -> AppResult<()> {
    let ts = now_ts();
    let affected = conn
        .execute(
            "UPDATE synthesis_jobs SET error_message = ?1, status = 'failed', completed_at = ?2 WHERE id = ?3",
            rusqlite::params![message, ts, id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("synthesis job {id}")));
    }
    Ok(())
}

/// Marks any jobs that were left pending/in_progress (e.g., app crashed)
/// as failed so the UI doesn't show eternal spinners.
pub fn fail_stale_jobs(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "UPDATE synthesis_jobs SET status = 'failed', error_message = 'App closed during synthesis' 
         WHERE status IN ('pending', 'in_progress')",
        [],
    )
    .map_err(map_db)?;
    Ok(())
}

/// Deletes a job.
pub fn delete_job(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM synthesis_jobs WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("synthesis job {id}")));
    }
    Ok(())
}
