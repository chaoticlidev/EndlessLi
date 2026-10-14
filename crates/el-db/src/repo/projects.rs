//! Project repository: the top-level organizer every note belongs to.

use el_core::{AppError, AppResult, NewProject, Project};
use rusqlite::{Connection, Row};

use super::{bool_param, check_not_blank, get_bool, map_db, new_id, now_ts};

const COLUMNS: &str =
    "id, name, description, icon, color, system_prompt, encrypted, archived, created_at";

fn row_to_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        icon: row.get(3)?,
        color: row.get(4)?,
        system_prompt: row.get(5)?,
        encrypted: get_bool(row, 6)?,
        archived: get_bool(row, 7)?,
        created_at: row.get(8)?,
    })
}

/// Creates a project. Names must not be blank.
pub fn create(conn: &Connection, input: &NewProject) -> AppResult<Project> {
    let name = check_not_blank(&input.name, "project name")?;
    let project = Project {
        id: new_id(),
        name,
        description: input.description.clone(),
        icon: input.icon.clone(),
        color: input.color.clone(),
        system_prompt: input.system_prompt.clone(),
        encrypted: false,
        archived: false,
        created_at: now_ts(),
    };
    conn.execute(
        "INSERT INTO projects(id, name, description, icon, color, system_prompt, encrypted, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7)",
        rusqlite::params![
            project.id,
            project.name,
            project.description,
            project.icon,
            project.color,
            project.system_prompt,
            project.created_at
        ],
    )
    .map_err(map_db)?;
    Ok(project)
}

/// Fetches a project by id, or `None` when it does not exist.
pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Project>> {
    let mut stmt = conn
        .prepare(&format!("SELECT {COLUMNS} FROM projects WHERE id = ?1"))
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_project(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Fetches a project by id, or returns `NotFound`.
pub fn require(conn: &Connection, id: &str) -> AppResult<Project> {
    get(conn, id)?.ok_or_else(|| AppError::NotFound(format!("project {id}")))
}

/// Lists projects oldest-first. Archived projects are excluded unless asked for.
pub fn list(conn: &Connection, include_archived: bool) -> AppResult<Vec<Project>> {
    let sql = if include_archived {
        format!("SELECT {COLUMNS} FROM projects ORDER BY created_at ASC")
    } else {
        format!("SELECT {COLUMNS} FROM projects WHERE archived = 0 ORDER BY created_at ASC")
    };
    let mut stmt = conn.prepare(&sql).map_err(map_db)?;
    let rows = stmt.query_map([], row_to_project).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Updates the mutable project fields.
pub fn update(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    description: Option<Option<&str>>,
    icon: Option<Option<&str>>,
    color: Option<Option<&str>>,
    system_prompt: Option<Option<&str>>,
) -> AppResult<Project> {
    let mut project = require(conn, id)?;
    if let Some(name) = name {
        project.name = check_not_blank(name, "project name")?;
    }
    if let Some(description) = description {
        project.description = description.map(str::to_string);
    }
    if let Some(icon) = icon {
        project.icon = icon.map(str::to_string);
    }
    if let Some(color) = color {
        project.color = color.map(str::to_string);
    }
    if let Some(system_prompt) = system_prompt {
        project.system_prompt = system_prompt.map(str::to_string);
    }
    let affected = conn
        .execute(
            "UPDATE projects SET name = ?1, description = ?2, icon = ?3, color = ?4, system_prompt = ?5 WHERE id = ?6",
            rusqlite::params![
                project.name,
                project.description,
                project.icon,
                project.color,
                project.system_prompt,
                id
            ],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("project {id}")));
    }
    Ok(project)
}

/// Archives or unarchives a project.
pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> AppResult<()> {
    let affected = conn
        .execute(
            "UPDATE projects SET archived = ?1 WHERE id = ?2",
            rusqlite::params![bool_param(archived), id],
        )
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("project {id}")));
    }
    Ok(())
}

/// Deletes a project. Folders cascade; notes keep their content with
/// `project_id` cleared to NULL by foreign-key actions.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM projects WHERE id = ?1", [id])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("project {id}")));
    }
    Ok(())
}
