//! Library repository: archived sources plus FTS search over them.

use el_core::{
    AppError, AppResult, LibraryItem, NewLibraryItem, Tag, UpdateLibraryItem, LIBRARY_TYPE_VALUES,
};
use rusqlite::{Connection, Row};

use super::{check_enum, check_not_blank, map_db, new_id, now_ts};

const COLUMNS: &str = "id, type, title, authors, url, source, publication_date, content, file_path, note_id, created_at";

fn row_to_item(row: &Row<'_>) -> rusqlite::Result<LibraryItem> {
    Ok(LibraryItem {
        id: row.get(0)?,
        item_type: row.get(1)?,
        title: row.get(2)?,
        authors: row.get(3)?,
        url: row.get(4)?,
        source: row.get(5)?,
        publication_date: row.get(6)?,
        content: row.get(7)?,
        file_path: row.get(8)?,
        note_id: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn row_to_tag(row: &Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
    })
}

fn collect_items(
    stmt: &mut rusqlite::Statement<'_>,
    params: &[&dyn rusqlite::ToSql],
) -> AppResult<Vec<LibraryItem>> {
    let rows = stmt.query_map(params, row_to_item).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Imports a library item. `authors` holds a JSON array when present.
pub fn create(conn: &Connection, input: &NewLibraryItem) -> AppResult<LibraryItem> {
    check_enum(&input.item_type, LIBRARY_TYPE_VALUES, "type")?;
    let title = check_not_blank(&input.title, "item title")?;
    let item = LibraryItem {
        id: new_id(),
        item_type: input.item_type.clone(),
        title,
        authors: input.authors.clone(),
        url: input.url.clone(),
        source: input.source.clone(),
        publication_date: input.publication_date.clone(),
        content: input.content.clone(),
        file_path: input.file_path.clone(),
        note_id: None,
        created_at: now_ts(),
    };
    conn.execute(
        "INSERT INTO library_items(id, type, title, authors, url, source, publication_date, content, file_path, note_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10)",
        rusqlite::params![
            item.id,
            item.item_type,
            item.title,
            item.authors,
            item.url,
            item.source,
            item.publication_date,
            item.content,
            item.file_path,
            item.created_at
        ],
    )
    .map_err(map_db)?;
    Ok(item)
}

/// Fetches an item by id, or `None` when it does not exist.
pub fn get(conn: &Connection, id: &str) -> AppResult<Option<LibraryItem>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM library_items WHERE id = ?1"
        ))
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_item(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Fetches an item by id, or returns `NotFound`.
pub fn require(conn: &Connection, id: &str) -> AppResult<LibraryItem> {
    get(conn, id)?.ok_or_else(|| AppError::NotFound(format!("library item {id}")))
}

/// Lists items newest-first, optionally filtered by type.
pub fn list(conn: &Connection, item_type: Option<&str>) -> AppResult<Vec<LibraryItem>> {
    if let Some(item_type) = item_type {
        check_enum(item_type, LIBRARY_TYPE_VALUES, "type")?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM library_items WHERE type = ?1 ORDER BY created_at DESC"
            ))
            .map_err(map_db)?;
        collect_items(&mut stmt, &[&item_type])
    } else {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM library_items ORDER BY created_at DESC"
            ))
            .map_err(map_db)?;
        collect_items(&mut stmt, &[])
    }
}

/// Updates an item's editable fields.
pub fn update(conn: &Connection, id: &str, patch: &UpdateLibraryItem) -> AppResult<LibraryItem> {
    let mut item = require(conn, id)?;
    if let Some(title) = &patch.title {
        item.title = check_not_blank(title, "item title")?;
    }
    if let Some(authors) = &patch.authors {
        item.authors = authors.clone();
    }
    if let Some(url) = &patch.url {
        item.url = url.clone();
    }
    if let Some(source) = &patch.source {
        item.source = source.clone();
    }
    if let Some(content) = &patch.content {
        item.content = content.clone();
    }
    if let Some(file_path) = &patch.file_path {
        item.file_path = file_path.clone();
    }
    if let Some(note_id) = &patch.note_id {
        item.note_id = note_id.clone();
    }
    let affected = conn
        .execute(
            "UPDATE library_items SET title = ?1, authors = ?2, url = ?3, source = ?4,
             content = ?5, file_path = ?6, note_id = ?7 WHERE id = ?8",
            rusqlite::params![
                item.title,
                item.authors,
                item.url,
                item.source,
                item.content,
                item.file_path,
                item.note_id,
                id
            ],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("library item {id}")));
    }
    Ok(item)
}

/// Deletes an item. Tag join rows cascade; the FTS row is removed by trigger.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM library_items WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("library item {id}")));
    }
    Ok(())
}

/// Full-text search over item titles and extracted content. The query is
/// wrapped as an FTS5 phrase so arbitrary input cannot cause syntax errors.
pub fn search(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<LibraryItem>> {
    let phrase = format!("\"{}\"", query.replace('"', ""));
    let prefixed = format!("i.{}", COLUMNS.replace(", ", ", i."));
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {prefixed} FROM library_items i JOIN library_fts f ON f.rowid = i.rowid
             WHERE library_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        ))
        .map_err(map_db)?;
    collect_items(
        &mut stmt,
        &[&phrase as &dyn rusqlite::ToSql, &(limit as i64)],
    )
}

/// Attaches a tag to an item. Repeat calls are harmless.
pub fn assign_tag(conn: &Connection, item_id: &str, tag_id: &str) -> AppResult<()> {
    match conn.execute(
        "INSERT INTO library_item_tags(item_id, tag_id) VALUES (?1, ?2)",
        (item_id, tag_id),
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

/// Detaches a tag from an item.
pub fn unassign_tag(conn: &Connection, item_id: &str, tag_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM library_item_tags WHERE item_id = ?1 AND tag_id = ?2",
        (item_id, tag_id),
    )
    .map_err(map_db)?;
    Ok(())
}

/// Lists the tags attached to an item, alphabetically.
pub fn tags_for_item(conn: &Connection, item_id: &str) -> AppResult<Vec<Tag>> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.color FROM tags t
             JOIN library_item_tags lt ON lt.tag_id = t.id
             WHERE lt.item_id = ?1 ORDER BY t.name COLLATE NOCASE ASC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([item_id], row_to_tag).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}
