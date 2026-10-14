//! Repositories: one module per entity group, single write paths.
//!
//! Conventions: read functions take `&Connection`, multi-statement writes
//! take `&mut Connection` and run inside one transaction. Every repository
//! maps SQLite constraint failures precisely: unique violations become
//! [`AppError::Conflict`], foreign-key violations become
//! [`AppError::InvalidInput`], everything else becomes
//! [`AppError::Database`]. Missing rows surface as `None` from `get`
//! functions or [`AppError::NotFound`] from `require` functions.

pub mod cache;
pub mod folders;
pub mod library;
pub mod links;
pub mod memory;
pub mod notes;
pub mod projects;
pub mod research;
pub mod skills;
pub mod tags;

use el_core::{AppError, AppResult};
use rusqlite::{Connection, Error as RusqliteError, Row};

/// Current Unix timestamp in seconds.
pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// New sortable unique identifier (UUID v7).
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Rejects blank values for required text columns.
pub fn check_not_blank(value: &str, field: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput(format!("{field} must not be blank")));
    }
    Ok(trimmed.to_string())
}

/// Rejects values outside a CHECK-constrained vocabulary.
pub fn check_enum(value: &str, allowed: &[&str], field: &str) -> AppResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "{field} must be one of {}",
            allowed.join(", ")
        )))
    }
}

/// Translates SQLite failures into application errors.
pub fn map_db(err: RusqliteError) -> AppError {
    match &err {
        RusqliteError::SqliteFailure(info, _) => {
            let code = info.extended_code;
            if code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
            {
                return AppError::Conflict(err.to_string());
            }
            if code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY {
                return AppError::InvalidInput(err.to_string());
            }
            AppError::Database(err.to_string())
        }
        RusqliteError::QueryReturnedNoRows => AppError::NotFound(err.to_string()),
        _ => AppError::Database(err.to_string()),
    }
}

/// Reads an INTEGER 0/1 column as bool.
pub fn get_bool(row: &Row<'_>, idx: usize) -> rusqlite::Result<bool> {
    Ok(row.get::<_, i64>(idx)? != 0)
}

/// Binds an optional bool as INTEGER for writes.
pub fn bool_param(value: bool) -> i64 {
    i64::from(value)
}

/// Executes a statement expected to touch exactly one row.
pub fn check_one_row(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> AppResult<()> {
    let affected = conn.execute(sql, params).map_err(map_db)?;
    if affected == 0 {
        return Err(AppError::NotFound("row not found".to_string()));
    }
    Ok(())
}

/// Converts a byte slice to lowercase hex (used for cache key digests).
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
    }
    out
}
