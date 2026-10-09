//! `el-db`: SQLite source of truth (Phase 1).
//!
//! Why this crate exists: all user data lives in one WAL-mode SQLite file.
//! This crate will own the connection pool, the `refinery`/`rusqlite_migration`
//! migration chain from Section 5 of the implementation plan, and one
//! repository module per entity. Nothing here depends on Tauri directly so
//! repositories stay unit-testable against an in-memory database.

// Placeholder re-export so downstream crates can depend on `el-db` from Phase 0.
pub use el_core::{AppError, AppResult};

/// Database schema version this crate will migrate to. Phase 1 sets it to 1.
pub const TARGET_SCHEMA_VERSION: u32 = 0;
