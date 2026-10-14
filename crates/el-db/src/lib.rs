//! `el-db`: SQLite source of truth.
//!
//! Owns the connection pool (WAL mode, foreign keys), the ordered migration
//! chain, one repository module per entity group, and the `sqlite-vec`
//! vector layer. Nothing here depends on Tauri, so repositories stay
//! testable against throwaway databases.

pub mod migrate;
pub mod pool;
pub mod repo;
pub mod vec;

pub use el_core::{AppError, AppResult};
pub use pool::{Database, DbConn};

/// Database schema version this crate migrates to.
pub const TARGET_SCHEMA_VERSION: u32 = migrate::TARGET_SCHEMA_VERSION;
