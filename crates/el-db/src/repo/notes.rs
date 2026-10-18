//! Note repository: CRUD, branching, and full-text search.
//!
//! The FTS index stays in sync through SQL triggers (see `0001_init.sql`),
//! so no repository call can forget to index a write.

use el_core::{AppError, AppResult, NewNote, Note, UpdateNote, NOTE_AUTHOR_VALUES};
use rusqlite::{Connection, Row};

use super::{check_enum, check_not_blank, map_db, new_id, now_ts};

const COLUMNS: &str = "id, title, content, project_id, folder_id, author, source_session_id, created_from_note_id, embedding_model, embedding_hash, created_at, updated_at";

pub(crate) fn row_to_note(row: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        project_id: row.get(3)?,
        folder_id: row.get(4)?,
        author: row.get(5)?,
        source_session_id: row.get(6)?,
        created_from_note_id: row.get(7)?,
        embedding_model: row.get(8)?,
        embedding_hash: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn collect_notes(
    stmt: &mut rusqlite::Statement<'_>,
    params: &[&dyn rusqlite::ToSql],
) -> AppResult<Vec<Note>> {
    let rows = stmt.query_map(params, row_to_note).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Creates a note. Unknown project, folder, or session ids fail with
/// `InvalidInput` through the foreign keys.
pub fn create(conn: &Connection, input: &NewNote) -> AppResult<Note> {
    let title = check_not_blank(&input.title, "note title")?;
    check_enum(&input.author, NOTE_AUTHOR_VALUES, "author")?;
    let now = now_ts();
    let note = Note {
        id: new_id(),
        title,
        content: input.content.clone(),
        project_id: input.project_id.clone(),
        folder_id: input.folder_id.clone(),
        author: input.author.clone(),
        source_session_id: input.source_session_id.clone(),
        created_from_note_id: input.created_from_note_id.clone(),
        embedding_model: None,
        embedding_hash: None,
        created_at: now,
        updated_at: now,
    };
    conn.execute(
        "INSERT INTO notes(id, title, content, project_id, folder_id, author, source_session_id, created_from_note_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            note.id,
            note.title,
            note.content,
            note.project_id,
            note.folder_id,
            note.author,
            note.source_session_id,
            note.created_from_note_id,
            note.created_at,
            note.updated_at
        ],
    )
    .map_err(map_db)?;
    Ok(note)
}

/// Fetches a note by id, or `None` when it does not exist.
pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Note>> {
    let mut stmt = conn
        .prepare(&format!("SELECT {COLUMNS} FROM notes WHERE id = ?1"))
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_note(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Fetches a note by id, or returns `NotFound`.
pub fn require(conn: &Connection, id: &str) -> AppResult<Note> {
    get(conn, id)?.ok_or_else(|| AppError::NotFound(format!("note {id}")))
}

/// Lists Inbox notes (no project), newest-first.
pub fn list_inbox(conn: &Connection) -> AppResult<Vec<Note>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM notes WHERE project_id IS NULL ORDER BY updated_at DESC"
        ))
        .map_err(map_db)?;
    collect_notes(&mut stmt, &[])
}

/// Lists the notes of one project, newest-first.
pub fn list_by_project(conn: &Connection, project_id: &str) -> AppResult<Vec<Note>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM notes WHERE project_id = ?1 ORDER BY updated_at DESC"
        ))
        .map_err(map_db)?;
    collect_notes(&mut stmt, &[&project_id])
}

/// Lists the notes of one folder, newest-first.
pub fn list_by_folder(conn: &Connection, folder_id: &str) -> AppResult<Vec<Note>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM notes WHERE folder_id = ?1 ORDER BY updated_at DESC"
        ))
        .map_err(map_db)?;
    collect_notes(&mut stmt, &[&folder_id])
}

/// Lists every note, newest-first. Used for global views and exports.
pub fn list_all(conn: &Connection) -> AppResult<Vec<Note>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM notes ORDER BY updated_at DESC"
        ))
        .map_err(map_db)?;
    collect_notes(&mut stmt, &[])
}

/// Finds notes by title for `[[wikilink]]` resolution: exact matches
/// (case-insensitive) first, then prefix matches, capped at `limit`.
pub fn find_by_title(conn: &Connection, title: &str, limit: usize) -> AppResult<Vec<Note>> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM notes WHERE title = ?1 COLLATE NOCASE
             UNION ALL
             SELECT {COLUMNS} FROM notes WHERE title LIKE ?2 ESCAPE '\\'
             AND title != ?1 COLLATE NOCASE
             LIMIT ?3"
        ))
        .map_err(map_db)?;
    let prefix = format!("{}%", trimmed.replace(['%', '_', '\\'], ""));
    collect_notes(
        &mut stmt,
        &[&trimmed as &dyn rusqlite::ToSql, &prefix, &(limit as i64)],
    )
}

/// Updates a note. When neither title nor content changed, the existing
/// embedding reference is kept; otherwise it is cleared so the note is
/// re-embedded on the next pass.
pub fn update(conn: &mut Connection, id: &str, patch: &UpdateNote) -> AppResult<Note> {
    let tx = conn
        .transaction()
        .map_err(|err| AppError::Database(err.to_string()))?;
    let mut note = require(&tx, id)?;
    let mut embedding_stale = false;
    if let Some(title) = &patch.title {
        note.title = check_not_blank(title, "note title")?;
        // Title feeds the FTS index and the embedding input; any title edit
        // conservatively marks the embedding stale.
        embedding_stale = true;
    }
    if let Some(content) = &patch.content {
        if *content != note.content {
            note.content.clone_from(content);
            embedding_stale = true;
        }
    }
    note.updated_at = now_ts();
    if embedding_stale {
        note.embedding_model = None;
        note.embedding_hash = None;
    }
    let affected = tx
        .execute(
            "UPDATE notes SET title = ?1, content = ?2, project_id = ?3, folder_id = ?4,
             embedding_model = ?5, embedding_hash = ?6, updated_at = ?7 WHERE id = ?8",
            rusqlite::params![
                note.title,
                note.content,
                note.project_id,
                note.folder_id,
                note.embedding_model,
                note.embedding_hash,
                note.updated_at,
                id
            ],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("note {id}")));
    }
    tx.commit()
        .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(note)
}

/// Moves a note to a project and folder. `None` unplaces it (Inbox for
/// projects, project root for folders). Unknown ids fail with `InvalidInput`.
pub fn move_note(
    conn: &Connection,
    id: &str,
    project_id: Option<&str>,
    folder_id: Option<&str>,
) -> AppResult<Note> {
    let mut note = require(conn, id)?;
    note.project_id = project_id.map(str::to_string);
    note.folder_id = folder_id.map(str::to_string);
    note.updated_at = now_ts();
    let affected = conn
        .execute(
            "UPDATE notes SET project_id = ?1, folder_id = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![note.project_id, note.folder_id, note.updated_at, id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("note {id}")));
    }
    Ok(note)
}

/// Copies a note (with its tags) into a new note that records its origin.
pub fn branch(conn: &mut Connection, id: &str, title: Option<&str>) -> AppResult<Note> {
    let tx = conn
        .transaction()
        .map_err(|err| AppError::Database(err.to_string()))?;
    let source = require(&tx, id)?;
    let now = now_ts();
    let branched = Note {
        id: new_id(),
        title: title.map_or_else(|| format!("{} (branch)", source.title), str::to_string),
        content: source.content.clone(),
        project_id: source.project_id.clone(),
        folder_id: source.folder_id.clone(),
        author: source.author.clone(),
        source_session_id: source.source_session_id.clone(),
        created_from_note_id: Some(source.id.clone()),
        embedding_model: None,
        embedding_hash: None,
        created_at: now,
        updated_at: now,
    };
    tx.execute(
        "INSERT INTO notes(id, title, content, project_id, folder_id, author, source_session_id, created_from_note_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            branched.id,
            branched.title,
            branched.content,
            branched.project_id,
            branched.folder_id,
            branched.author,
            branched.source_session_id,
            branched.created_from_note_id,
            branched.created_at,
            branched.updated_at
        ],
    )
    .map_err(map_db)?;
    tx.execute(
        "INSERT INTO note_tags(note_id, tag_id) SELECT ?1, tag_id FROM note_tags WHERE note_id = ?2",
        (&branched.id, &source.id),
    )
    .map_err(map_db)?;
    tx.commit()
        .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(branched)
}

/// Deletes a note. Links, tags, vectors, and session attachments are removed
/// by cascades and cleanup below; the FTS row is removed by trigger.
pub fn delete(conn: &mut Connection, id: &str) -> AppResult<()> {
    let tx = conn
        .transaction()
        .map_err(|err| AppError::Database(err.to_string()))?;
    crate::vec::delete_embedding(&tx, id)?;
    let affected = tx
        .execute("DELETE FROM notes WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("note {id}")));
    }
    tx.commit()
        .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(())
}

/// Counts notes embedded with `model` versus the total note count. Drives the
/// Settings embedding-coverage display and the re-embed job.
pub fn embedding_counts(conn: &Connection, model: &str) -> AppResult<(i64, i64)> {
    conn.query_row(
        "SELECT
           COALESCE(SUM(CASE WHEN embedding_model = ?1 THEN 1 ELSE 0 END), 0),
           COUNT(*)
         FROM notes",
        [model],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(map_db)
}

/// Lists ids of notes that still need embedding with `model` (never embedded,
/// or embedded with a different model), oldest first, capped at `limit`.
pub fn list_needing_embedding(
    conn: &Connection,
    model: &str,
    limit: usize,
) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM notes
             WHERE embedding_model IS NULL OR embedding_model != ?1
             ORDER BY updated_at ASC LIMIT ?2",
        )
        .map_err(map_db)?;
    let rows = stmt
        .query_map(rusqlite::params![model, limit as i64], |row| row.get(0))
        .map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Full-text search over titles and content, newest matches first.
///
/// The query is wrapped as an FTS5 phrase (quotes stripped) so arbitrary user
/// input can never produce a syntax error.
pub fn search(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<Note>> {
    let phrase = format!("\"{}\"", query.replace('"', ""));
    let prefixed = format!("n.{}", COLUMNS.replace(", ", ", n."));
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {prefixed} FROM notes n JOIN notes_fts f ON f.rowid = n.rowid
             WHERE notes_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        ))
        .map_err(map_db)?;
    collect_notes(
        &mut stmt,
        &[&phrase as &dyn rusqlite::ToSql, &(limit as i64)],
    )
}
