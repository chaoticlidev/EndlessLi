//! Connection pool: WAL mode, foreign keys, busy timeout, one write path.
//!
//! SQLite allows a single writer, so every repository goes through this pool
//! and configures each connection identically on open. WAL mode and foreign
//! keys are verified after opening so a misconfigured connection can never
//! silently serve traffic.

use std::path::{Path, PathBuf};
use std::time::Duration;

use el_core::{AppError, AppResult};
use r2d2::{ManageConnection, Pool, PooledConnection};
use rusqlite::Connection;

use crate::migrate;

/// Pooled SQLite connection.
pub type DbConn = PooledConnection<SqliteManager>;

/// How long a connection waits on a locked database before failing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Connection factory for the `r2d2` pool. Kept private so every connection
/// is created with identical pragmas.
#[derive(Debug, Clone)]
pub struct SqliteManager {
    path: PathBuf,
}

impl ManageConnection for SqliteManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(&self.path)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(conn)
    }

    fn is_valid(&self, conn: &mut Connection) -> Result<(), rusqlite::Error> {
        conn.query_row("SELECT 1", [], |_| Ok(()))
    }

    fn has_broken(&self, _conn: &mut Connection) -> bool {
        false
    }
}

/// The database: a pooled handle plus the file it was opened from.
#[derive(Debug, Clone)]
pub struct Database {
    pool: Pool<SqliteManager>,
    path: PathBuf,
}

impl Database {
    /// Opens (creating parent directories) the database at `path`, registers
    /// the vector extension, runs pending migrations, and verifies WAL mode.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| AppError::Database(format!("create db dir: {err}")))?;
            }
        }
        crate::vec::register_extension().map_err(AppError::Internal)?;
        let manager = SqliteManager {
            path: path.to_path_buf(),
        };
        let pool = Pool::builder()
            .max_size(8)
            .build(manager)
            .map_err(|err| AppError::Database(format!("build pool: {err}")))?;
        let db = Self {
            pool,
            path: path.to_path_buf(),
        };
        let mut conn = db.connect()?;
        migrate::run(&mut conn)?;
        db.verify_pragmas(&conn)?;
        tracing::info!(path = %path.display(), "database opened");
        Ok(db)
    }

    /// Opens a throwaway database in the system temp directory for tests.
    /// The caller owns the returned path and removes the file when done.
    pub fn open_ephemeral() -> AppResult<(Self, PathBuf)> {
        let path = std::env::temp_dir().join(format!("endlessli-test-{}.db", uuid::Uuid::now_v7()));
        let db = Self::open(&path)?;
        Ok((db, path))
    }

    /// File this database was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Checks out a pooled connection.
    pub fn connect(&self) -> AppResult<DbConn> {
        self.pool
            .get()
            .map_err(|err| AppError::Database(format!("pool checkout: {err}")))
    }

    /// Runs `f` inside a transaction on a single pooled connection.
    /// Any error rolls the transaction back.
    pub fn transaction<T, F>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> AppResult<T>,
    {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|err| AppError::Database(err.to_string()))?;
        let out = f(&tx)?;
        tx.commit()
            .map_err(|err| AppError::Database(err.to_string()))?;
        Ok(out)
    }

    fn verify_pragmas(&self, conn: &Connection) -> AppResult<()> {
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(|err| AppError::Database(err.to_string()))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(AppError::Database(format!(
                "journal_mode is {mode}, expected WAL"
            )));
        }
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|err| AppError::Database(err.to_string()))?;
        if fk != 1 {
            return Err(AppError::Database("foreign_keys is off".to_string()));
        }
        Ok(())
    }
}
