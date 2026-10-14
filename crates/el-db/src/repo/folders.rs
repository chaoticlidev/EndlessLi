//! Folder repository: nested, project-scoped organization.

use el_core::{AppError, AppResult, Folder, NewFolder};
use rusqlite::{Connection, Row};

use super::{check_not_blank, map_db, new_id, now_ts};

const COLUMNS: &str = "id, name, project_id, parent_id, created_at";

fn row_to_folder(row: &Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        name: row.get(1)?,
        project_id: row.get(2)?,
        parent_id: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Creates a folder. The project must exist; the parent, when given, must
/// belong to the same project. Foreign-key violations become `InvalidInput`.
pub fn create(conn: &Connection, input: &NewFolder) -> AppResult<Folder> {
    let name = check_not_blank(&input.name, "folder name")?;
    if let Some(parent_id) = &input.parent_id {
        let parent = require(conn, parent_id)?;
        if parent.project_id != input.project_id {
            return Err(AppError::InvalidInput(
                "parent folder belongs to a different project".to_string(),
            ));
        }
    }
    let folder = Folder {
        id: new_id(),
        name,
        project_id: input.project_id.clone(),
        parent_id: input.parent_id.clone(),
        created_at: now_ts(),
    };
    conn.execute(
        "INSERT INTO folders(id, name, project_id, parent_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            folder.id,
            folder.name,
            folder.project_id,
            folder.parent_id,
            folder.created_at
        ],
    )
    .map_err(map_db)?;
    Ok(folder)
}

/// Fetches a folder by id, or `None` when it does not exist.
pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Folder>> {
    let mut stmt = conn
        .prepare(&format!("SELECT {COLUMNS} FROM folders WHERE id = ?1"))
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_folder(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Fetches a folder by id, or returns `NotFound`.
pub fn require(conn: &Connection, id: &str) -> AppResult<Folder> {
    get(conn, id)?.ok_or_else(|| AppError::NotFound(format!("folder {id}")))
}

/// Lists every folder of a project, oldest-first. Callers assemble trees.
pub fn list_by_project(conn: &Connection, project_id: &str) -> AppResult<Vec<Folder>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM folders WHERE project_id = ?1 ORDER BY created_at ASC"
        ))
        .map_err(map_db)?;
    let rows = stmt
        .query_map([project_id], row_to_folder)
        .map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Renames a folder.
pub fn rename(conn: &Connection, id: &str, name: &str) -> AppResult<Folder> {
    let mut folder = require(conn, id)?;
    folder.name = check_not_blank(name, "folder name")?;
    let affected = conn
        .execute(
            "UPDATE folders SET name = ?1 WHERE id = ?2",
            (&folder.name, id),
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("folder {id}")));
    }
    Ok(folder)
}

/// Moves a folder under a new parent (or to the project root with `None`).
/// Moves that would create a cycle or cross projects are rejected.
pub fn move_folder(conn: &Connection, id: &str, new_parent: Option<&str>) -> AppResult<Folder> {
    let mut folder = require(conn, id)?;
    if let Some(parent_id) = new_parent {
        if parent_id == id {
            return Err(AppError::InvalidInput(
                "folder cannot be its own parent".to_string(),
            ));
        }
        let parent = require(conn, parent_id)?;
        if parent.project_id != folder.project_id {
            return Err(AppError::InvalidInput(
                "parent folder belongs to a different project".to_string(),
            ));
        }
        if is_descendant(conn, parent_id, id)? {
            return Err(AppError::InvalidInput(
                "move would create a folder cycle".to_string(),
            ));
        }
        folder.parent_id = Some(parent_id.to_string());
    } else {
        folder.parent_id = None;
    }
    let affected = conn
        .execute(
            "UPDATE folders SET parent_id = ?1 WHERE id = ?2",
            rusqlite::params![folder.parent_id, id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("folder {id}")));
    }
    Ok(folder)
}

/// Deletes a folder. Nested folders cascade; member notes keep their content
/// with `folder_id` cleared to NULL by foreign-key actions.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM folders WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("folder {id}")));
    }
    Ok(())
}

/// Reports whether `candidate` is `ancestor` itself or sits below it.
fn is_descendant(conn: &Connection, candidate: &str, ancestor: &str) -> AppResult<bool> {
    let mut current = Some(candidate.to_string());
    while let Some(id) = current {
        if id == ancestor {
            return Ok(true);
        }
        current = get(conn, &id)?.and_then(|folder| folder.parent_id);
    }
    Ok(false)
}
