//! Skills and scripts repository: the two tiers of reusable capabilities.

use el_core::{AppError, AppResult, NewScript, NewSkill, Script, Skill};
use rusqlite::{Connection, Row};

use super::{bool_param, check_not_blank, get_bool, map_db, new_id, now_ts};

fn row_to_skill(row: &Row<'_>) -> rusqlite::Result<Skill> {
    Ok(Skill {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        content: row.get(3)?,
        parameters: row.get(4)?,
        category: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn row_to_script(row: &Row<'_>) -> rusqlite::Result<Script> {
    Ok(Script {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        content: row.get(3)?,
        parameters: row.get(4)?,
        category: row.get(5)?,
        allow_network: get_bool(row, 6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// Saves a skill, replacing any skill with the same name.
pub fn save_skill(conn: &Connection, input: &NewSkill) -> AppResult<Skill> {
    let name = check_not_blank(&input.name, "skill name")?;
    let now = now_ts();
    let existing: Option<Skill> = get_skill_by_name(conn, &name)?;
    let skill = Skill {
        id: existing
            .as_ref()
            .map_or_else(new_id, |skill| skill.id.clone()),
        name,
        description: input.description.clone(),
        content: input.content.clone(),
        parameters: input.parameters.clone(),
        category: input.category.clone(),
        created_at: existing.as_ref().map_or(now, |skill| skill.created_at),
        updated_at: now,
    };
    conn.execute(
        "INSERT INTO skills(id, name, description, content, parameters, category, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(name) DO UPDATE SET description = excluded.description, content = excluded.content,
         parameters = excluded.parameters, category = excluded.category, updated_at = excluded.updated_at",
        rusqlite::params![
            skill.id,
            skill.name,
            skill.description,
            skill.content,
            skill.parameters,
            skill.category,
            skill.created_at,
            skill.updated_at
        ],
    )
    .map_err(map_db)?;
    Ok(skill)
}

/// Fetches a skill by name, or `None` when it does not exist.
pub fn get_skill_by_name(conn: &Connection, name: &str) -> AppResult<Option<Skill>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, content, parameters, category, created_at, updated_at
             FROM skills WHERE name = ?1",
        )
        .map_err(map_db)?;
    let mut rows = stmt.query([name]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_skill(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Lists skills alphabetically.
pub fn list_skills(conn: &Connection) -> AppResult<Vec<Skill>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, content, parameters, category, created_at, updated_at
             FROM skills ORDER BY name ASC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([], row_to_skill).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Deletes a skill by name.
pub fn delete_skill(conn: &Connection, name: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM skills WHERE name = ?1", [name])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("skill {name}")));
    }
    Ok(())
}

/// Saves a script, replacing any script with the same name.
pub fn save_script(conn: &Connection, input: &NewScript) -> AppResult<Script> {
    let name = check_not_blank(&input.name, "script name")?;
    let now = now_ts();
    let existing: Option<Script> = get_script_by_name(conn, &name)?;
    let script = Script {
        id: existing
            .as_ref()
            .map_or_else(new_id, |script| script.id.clone()),
        name,
        description: input.description.clone(),
        content: input.content.clone(),
        parameters: input.parameters.clone(),
        category: input.category.clone(),
        allow_network: input.allow_network,
        created_at: existing.as_ref().map_or(now, |script| script.created_at),
        updated_at: now,
    };
    conn.execute(
        "INSERT INTO scripts(id, name, description, content, parameters, category, allow_network, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(name) DO UPDATE SET description = excluded.description, content = excluded.content,
         parameters = excluded.parameters, category = excluded.category,
         allow_network = excluded.allow_network, updated_at = excluded.updated_at",
        rusqlite::params![
            script.id,
            script.name,
            script.description,
            script.content,
            script.parameters,
            script.category,
            bool_param(script.allow_network),
            script.created_at,
            script.updated_at
        ],
    )
    .map_err(map_db)?;
    Ok(script)
}

/// Fetches a script by name, or `None` when it does not exist.
pub fn get_script_by_name(conn: &Connection, name: &str) -> AppResult<Option<Script>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, content, parameters, category, allow_network, created_at, updated_at
             FROM scripts WHERE name = ?1",
        )
        .map_err(map_db)?;
    let mut rows = stmt.query([name]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_script(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Lists scripts alphabetically.
pub fn list_scripts(conn: &Connection) -> AppResult<Vec<Script>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, content, parameters, category, allow_network, created_at, updated_at
             FROM scripts ORDER BY name ASC",
        )
        .map_err(map_db)?;
    let rows = stmt.query_map([], row_to_script).map_err(map_db)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_db)?);
    }
    Ok(out)
}

/// Deletes a script by name.
pub fn delete_script(conn: &Connection, name: &str) -> AppResult<()> {
    let affected = conn
        .execute("DELETE FROM scripts WHERE name = ?1", [name])
        .map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("script {name}")));
    }
    Ok(())
}
