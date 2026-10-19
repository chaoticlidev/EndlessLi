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
        item.authors = Some(authors.clone());
    }
    if let Some(url) = &patch.url {
        item.url = Some(url.clone());
    }
    if let Some(source) = &patch.source {
        item.source = Some(source.clone());
    }
    if let Some(publication_date) = &patch.publication_date {
        item.publication_date = Some(publication_date.clone());
    }
    if let Some(content) = &patch.content {
        item.content = Some(content.clone());
    }
    if let Some(file_path) = &patch.file_path {
        item.file_path = Some(file_path.clone());
    }
    if let Some(note_id) = &patch.note_id {
        item.note_id = Some(note_id.clone());
    }
    let affected = conn
        .execute(
            "UPDATE library_items SET title = ?1, authors = ?2, url = ?3, source = ?4,
             publication_date = ?5, content = ?6, file_path = ?7, note_id = ?8 WHERE id = ?9",
            rusqlite::params![
                item.title,
                item.authors,
                item.url,
                item.source,
                item.publication_date,
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
    search_filtered(conn, Some(query), None, None, None, limit)
}

/// Lists or searches items with any combination of query, type, tag, and
/// source filters.
///
/// An empty/whitespace `query` lists newest-first; otherwise results are
/// ranked by FTS relevance. The query is wrapped as an FTS5 phrase (quotes
/// stripped) so arbitrary user input can never produce a syntax error. Type
/// is validated against the CHECK vocabulary; tag and source are exact-match
/// filters. Every value is bound, so the dynamically built SQL is safe.
pub fn search_filtered(
    conn: &Connection,
    query: Option<&str>,
    item_type: Option<&str>,
    tag_id: Option<&str>,
    source: Option<&str>,
    limit: usize,
) -> AppResult<Vec<LibraryItem>> {
    let query = query.map(str::trim).filter(|value| !value.is_empty());
    if let Some(item_type) = item_type {
        check_enum(item_type, LIBRARY_TYPE_VALUES, "type")?;
    }

    let prefixed = format!("i.{}", COLUMNS.replace(", ", ", i."));
    let mut sql = format!("SELECT {prefixed} FROM library_items i");
    if tag_id.is_some() {
        sql.push_str(" JOIN library_item_tags lit ON lit.item_id = i.id");
    }
    if query.is_some() {
        sql.push_str(" JOIN library_fts f ON f.rowid = i.rowid");
    }

    let mut wheres: Vec<&str> = Vec::new();
    if query.is_some() {
        wheres.push("library_fts MATCH ?");
    }
    if item_type.is_some() {
        wheres.push("i.type = ?");
    }
    if tag_id.is_some() {
        wheres.push("lit.tag_id = ?");
    }
    if source.is_some() {
        wheres.push("i.source = ?");
    }
    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    if query.is_some() {
        sql.push_str(" ORDER BY rank");
    } else {
        sql.push_str(" ORDER BY i.created_at DESC");
    }
    sql.push_str(" LIMIT ?");

    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(query) = query {
        binds.push(Box::new(format!("\"{}\"", query.replace('"', ""))));
    }
    if let Some(item_type) = item_type {
        binds.push(Box::new(item_type.to_string()));
    }
    if let Some(tag_id) = tag_id {
        binds.push(Box::new(tag_id.to_string()));
    }
    if let Some(source) = source {
        binds.push(Box::new(source.to_string()));
    }
    binds.push(Box::new(limit as i64));

    let mut stmt = conn.prepare(&sql).map_err(map_db)?;
    let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|value| value.as_ref()).collect();
    collect_items(&mut stmt, &bind_refs)
}

/// Counts items per type, for the Library categories panel.
pub fn type_counts(conn: &Connection) -> AppResult<Vec<(String, i64)>> {
    let mut stmt = conn
        .prepare("SELECT type, COUNT(*) FROM library_items GROUP BY type")
        .map_err(map_db)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Distinct, non-empty source names, alphabetically (case-insensitive).
pub fn sources(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT source FROM library_items
             WHERE source IS NOT NULL AND TRIM(source) != ''
             ORDER BY source COLLATE NOCASE ASC",
        )
        .map_err(map_db)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Fetches an item by exact URL, or `None`. Used to keep source imports
/// idempotent.
pub fn get_by_url(conn: &Connection, url: &str) -> AppResult<Option<LibraryItem>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM library_items WHERE url = ?1 ORDER BY created_at ASC LIMIT 1"
        ))
        .map_err(map_db)?;
    let mut rows = stmt.query([url]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_item(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Sets (or clears) the note linked to an item. Unknown note ids fail with
/// `InvalidInput` through the foreign key.
pub fn set_note(conn: &Connection, id: &str, note_id: Option<&str>) -> AppResult<LibraryItem> {
    let mut item = require(conn, id)?;
    item.note_id = note_id.map(str::to_string);
    let affected = conn
        .execute(
            "UPDATE library_items SET note_id = ?1 WHERE id = ?2",
            rusqlite::params![item.note_id, id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("library item {id}")));
    }
    Ok(item)
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
