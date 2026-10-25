//! Structured logging with a queryable SQLite layer.
//!
//! Why this exists: Phase 0 requires logs queryable in `logs.db`. A
//! `tracing` fmt layer keeps console output during development, while
//! [`SqliteLogLayer`] persists every event to a `logs` table so support
//! and diagnostics can query history with SQL instead of scraping files.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::Connection;
use tracing::field::{Field, Visit};
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

/// SQLite file name for application logs.
pub const LOG_DB_FILENAME: &str = "logs.db";

/// Delete log rows older than this on startup.
const LOG_RETENTION_SECS: i64 = 14 * 24 * 3600;

/// Keep at most this many rows even when they are recent.
const LOG_MAX_ROWS: i64 = 5_000;

/// Visitor that renders event fields into a single message string.
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            let _ = write!(self.message, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.message, " {}={value:?}", field.name());
        }
    }
}

/// A `tracing` layer that appends events to a SQLite `logs` table.
#[derive(Debug, Clone)]
pub struct SqliteLogLayer {
    db: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl SqliteLogLayer {
    /// Opens (creating parent directories) the log database and ensures schema.
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|err| format!("create log dir: {err}"))?;
        let db_path = dir.join(LOG_DB_FILENAME);
        let db = Connection::open(&db_path).map_err(|err| format!("open logs.db: {err}"))?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS logs (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               ts INTEGER NOT NULL,
               level TEXT NOT NULL,
               target TEXT NOT NULL,
               message TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_logs_ts ON logs(ts);",
        )
        .map_err(|err| format!("init logs schema: {err}"))?;
        // Bounded retention: logs are diagnostics, not user data.
        let cutoff = chrono::Utc::now().timestamp() - LOG_RETENTION_SECS;
        db.execute("DELETE FROM logs WHERE ts < ?1", [cutoff])
            .map_err(|err| format!("prune old logs: {err}"))?;
        db.execute(
            "DELETE FROM logs WHERE id NOT IN
             (SELECT id FROM logs ORDER BY id DESC LIMIT ?1)",
            [LOG_MAX_ROWS],
        )
        .map_err(|err| format!("prune excess logs: {err}"))?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            db_path,
        })
    }

    /// Path of the underlying database file (surfaced for diagnostics).
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn append(&self, level: &str, target: &str, message: &str) {
        let now = chrono::Utc::now().timestamp();
        if let Ok(db) = self.db.lock() {
            let _ = db.execute(
                "INSERT INTO logs (ts, level, target, message) VALUES (?1, ?2, ?3, ?4)",
                (now, level, target, message),
            );
        }
    }
}

impl<S> Layer<S> for SqliteLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);
        self.append(
            event.metadata().level().as_str(),
            event.metadata().target(),
            visitor.message.trim(),
        );
    }
}

/// Installs the global subscriber: console fmt layer plus the SQLite layer.
///
/// Calling more than once is a no-op returning the existing database path.
pub fn init_logging(data_dir: &Path) -> Result<PathBuf, String> {
    let sqlite = SqliteLogLayer::open(data_dir)?;
    let db_path = sqlite.db_path().to_path_buf();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(sqlite)
        .try_init();
    tracing::info!(path = %db_path.display(), "logging initialized");
    Ok(db_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::info;

    #[test]
    fn events_are_queryable_in_sqlite() {
        let dir = std::env::temp_dir().join(format!("endlessli-logs-{}", uuid::Uuid::now_v7()));
        let layer = SqliteLogLayer::open(&dir).expect("open");
        let subscriber = tracing_subscriber::registry().with(layer.clone());
        tracing::subscriber::with_default(subscriber, || {
            info!("hello sqlite layer");
        });
        let db = layer.db.lock().expect("lock");
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM logs WHERE message LIKE '%hello sqlite layer%'",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(count, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn old_and_excess_rows_are_pruned_on_open() {
        let dir = std::env::temp_dir().join(format!("endlessli-logs-{}", uuid::Uuid::now_v7()));
        let layer = SqliteLogLayer::open(&dir).expect("open");
        let old = chrono::Utc::now().timestamp() - LOG_RETENTION_SECS - 60;
        {
            let db = layer.db.lock().expect("lock");
            db.execute(
                "INSERT INTO logs (ts, level, target, message) VALUES (?1, 'INFO', 'test', 'old')",
                [old],
            )
            .expect("insert old");
            for index in 0..(LOG_MAX_ROWS + 10) {
                db.execute(
                    "INSERT INTO logs (ts, level, target, message) VALUES (?1, 'INFO', 'test', ?2)",
                    (chrono::Utc::now().timestamp(), format!("recent {index}")),
                )
                .expect("insert recent");
            }
        }
        drop(layer);
        let reopened = SqliteLogLayer::open(&dir).expect("reopen");
        let db = reopened.db.lock().expect("lock");
        let old_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM logs WHERE message = 'old'",
                [],
                |row| row.get(0),
            )
            .expect("count old");
        let total: i64 = db
            .query_row("SELECT COUNT(*) FROM logs", [], |row| row.get(0))
            .expect("count all");
        assert_eq!(old_count, 0);
        assert_eq!(total, LOG_MAX_ROWS);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }
}
