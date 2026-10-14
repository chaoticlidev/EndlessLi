//! Shared setup for `el-db` integration tests: one throwaway database file
//! per test, migrated to the latest schema.

use el_db::{Database, DbConn};
use std::path::PathBuf;

pub struct TestDb {
    pub db: Database,
    path: PathBuf,
}

impl TestDb {
    pub fn setup() -> Self {
        let (db, path) = Database::open_ephemeral().expect("open test database");
        Self { db, path }
    }

    pub fn conn(&self) -> DbConn {
        self.db.connect().expect("pool checkout")
    }

    /// Drops the pool first (releasing the Windows file lock), then removes
    /// the file. Leaks the file if the test panicked before this runs, which
    /// is harmless inside the system temp directory.
    pub fn finish(self) {
        drop(self.db);
        std::fs::remove_file(&self.path).ok();
    }
}
