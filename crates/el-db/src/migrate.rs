//! Schema migrations: plain SQL files applied in order via `user_version`.
//!
//! Migration `0001_init.sql` holds the full v1 schema. Later phases append
//! `0002_*.sql` files and entries in `MIGRATIONS_SLICE`; nothing else changes.

use el_core::{AppError, AppResult};
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

const MIGRATIONS_SLICE: &[M<'_>] = &[
    M::up(include_str!("../migrations/0001_init.sql")),
    M::up(include_str!("../migrations/0002_synthesis.sql")),
];

const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

/// Schema version a freshly migrated database reports.
pub const TARGET_SCHEMA_VERSION: u32 = 2;

/// Applies pending migrations atomically. Safe to call on every open.
pub fn run(conn: &mut Connection) -> AppResult<()> {
    MIGRATIONS
        .to_latest(conn)
        .map_err(|err| AppError::Database(format!("migrate: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid_and_reach_target_version() {
        assert!(MIGRATIONS.validate().is_ok());
        let mut conn = Connection::open_in_memory().expect("memory db");
        run(&mut conn).expect("migrate");
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, TARGET_SCHEMA_VERSION);
    }
}
