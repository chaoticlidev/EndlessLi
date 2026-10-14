//! Note-link repository: the single write path for bidirectional links.
//!
//! Each pair is stored exactly once with `note_a < note_b`, enforced by both
//! this module (which canonicalizes every call) and the SQL `CHECK`. Because
//! every link is one `INSERT`, an orphaned half-link cannot exist.

use el_core::{AppError, AppResult, NoteLink, LINK_ORIGIN_VALUES};
use rusqlite::{Connection, Row};

use super::{check_enum, map_db, now_ts};

fn row_to_link(row: &Row<'_>) -> rusqlite::Result<NoteLink> {
    Ok(NoteLink {
        note_a: row.get(0)?,
        note_b: row.get(1)?,
        similarity: row.get(2)?,
        origin: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Orders a pair canonically. Self-links are rejected.
pub fn canonical(a: &str, b: &str) -> AppResult<(String, String)> {
    if a == b {
        return Err(AppError::InvalidInput(
            "cannot link a note to itself".to_string(),
        ));
    }
    if a < b {
        Ok((a.to_string(), b.to_string()))
    } else {
        Ok((b.to_string(), a.to_string()))
    }
}

/// Records an approved link (auto-suggested or manual). Duplicate pairs
/// become `Conflict`; unknown notes become `InvalidInput`.
pub fn approve(
    conn: &Connection,
    a: &str,
    b: &str,
    similarity: Option<f64>,
    origin: &str,
) -> AppResult<NoteLink> {
    check_enum(origin, LINK_ORIGIN_VALUES, "origin")?;
    let (note_a, note_b) = canonical(a, b)?;
    let link = NoteLink {
        note_a,
        note_b,
        similarity,
        origin: origin.to_string(),
        created_at: now_ts(),
    };
    conn.execute(
        "INSERT INTO note_links(note_a, note_b, similarity, origin, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            link.note_a,
            link.note_b,
            link.similarity,
            link.origin,
            link.created_at
        ],
    )
    .map_err(map_db)?;
    Ok(link)
}

/// Records a manually created link.
pub fn create_manual(
    conn: &Connection,
    a: &str,
    b: &str,
    similarity: Option<f64>,
) -> AppResult<NoteLink> {
    approve(conn, a, b, similarity, "manual")
}

/// Records a rejection so the pair is never suggested again. Idempotent.
pub fn reject(conn: &Connection, a: &str, b: &str) -> AppResult<()> {
    let (note_a, note_b) = canonical(a, b)?;
    conn.execute(
        "INSERT OR IGNORE INTO link_rejections(note_a, note_b) VALUES (?1, ?2)",
        (note_a, note_b),
    )
    .map_err(map_db)?;
    Ok(())
}

/// Removes a link. Succeeds silently when the pair was never linked.
pub fn remove(conn: &Connection, a: &str, b: &str) -> AppResult<()> {
    let (note_a, note_b) = canonical(a, b)?;
    conn.execute(
        "DELETE FROM note_links WHERE note_a = ?1 AND note_b = ?2",
        (note_a, note_b),
    )
    .map_err(map_db)?;
    Ok(())
}

/// Lists every link touching a note, in either direction.
pub fn list_for_note(conn: &Connection, note_id: &str) -> AppResult<Vec<NoteLink>> {
    let mut stmt = conn
        .prepare(
            "SELECT note_a, note_b, similarity, origin, created_at FROM note_links
             WHERE note_a = ?1 OR note_b = ?1 ORDER BY created_at ASC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([note_id], row_to_link).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}
