//! Tag repository: case-insensitively unique labels for notes and items.

use el_core::{AppError, AppResult, Tag};
use rusqlite::{Connection, Row};

use super::{check_not_blank, map_db, new_id};

fn row_to_tag(row: &Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
    })
}

/// Creates a tag. Duplicate names (any case) become `Conflict`.
pub fn create(conn: &Connection, name: &str, color: Option<&str>) -> AppResult<Tag> {
    let name = check_not_blank(name, "tag name")?;
    let tag = Tag {
        id: new_id(),
        name,
        color: color.map(str::to_string),
    };
    conn.execute(
        "INSERT INTO tags(id, name, color) VALUES (?1, ?2, ?3)",
        rusqlite::params![tag.id, tag.name, tag.color],
    )
    .map_err(map_db)?;
    Ok(tag)
}

/// Fetches an existing tag (any case) or creates it. One row per name.
pub fn get_or_create(conn: &Connection, name: &str) -> AppResult<Tag> {
    if let Some(tag) = get_by_name(conn, name)? {
        return Ok(tag);
    }
    match create(conn, name, None) {
        Ok(tag) => Ok(tag),
        Err(AppError::Conflict(_)) => get_by_name(conn, name)?
            .ok_or_else(|| AppError::Database(format!("tag {name} vanished after conflict"))),
        Err(other) => Err(other),
    }
}

/// Fetches a tag by name, case-insensitively.
pub fn get_by_name(conn: &Connection, name: &str) -> AppResult<Option<Tag>> {
    let mut stmt = conn
        .prepare("SELECT id, name, color FROM tags WHERE name = ?1 COLLATE NOCASE")
        .map_err(map_db)?;
    let mut rows = stmt.query([name]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_tag(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Fetches a tag by id, or `None` when it does not exist.
pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Tag>> {
    let mut stmt = conn
        .prepare("SELECT id, name, color FROM tags WHERE id = ?1")
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_tag(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Lists every tag alphabetically (case-insensitive).
pub fn list(conn: &Connection) -> AppResult<Vec<Tag>> {
    let mut stmt = conn
        .prepare("SELECT id, name, color FROM tags ORDER BY name COLLATE NOCASE ASC")
        .map_err(map_db)?;
    let rows = stmt.query_map([], row_to_tag).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Deletes a tag. Join rows in `note_tags` and `library_item_tags` cascade.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM tags WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("tag {id}")));
    }
    Ok(())
}

/// Attaches a tag to a note. Repeat calls are harmless. Unknown note or tag
/// ids fail with `InvalidInput` through the foreign key.
pub fn assign_to_note(conn: &Connection, note_id: &str, tag_id: &str) -> AppResult<()> {
    match conn.execute(
        "INSERT INTO note_tags(note_id, tag_id) VALUES (?1, ?2)",
        (note_id, tag_id),
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

/// Detaches a tag from a note.
pub fn unassign_from_note(conn: &Connection, note_id: &str, tag_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM note_tags WHERE note_id = ?1 AND tag_id = ?2",
        (note_id, tag_id),
    )
    .map_err(map_db)?;
    Ok(())
}

/// Lists the tags attached to a note, alphabetically.
pub fn tags_for_note(conn: &Connection, note_id: &str) -> AppResult<Vec<Tag>> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.color FROM tags t
             JOIN note_tags nt ON nt.tag_id = t.id
             WHERE nt.note_id = ?1 ORDER BY t.name COLLATE NOCASE ASC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([note_id], row_to_tag).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}
