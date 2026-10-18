//! `el-analytics`: DuckDB analytical store for Semantic Compression.
//!
//! Why this crate exists: SQLite is the transactional source of truth, but
//! K-Means input preparation and large aggregations belong in a columnar
//! engine. This crate stays behind the `duckdb` Cargo feature until Phase 6
//! so CI and local builds pay no compile-time cost before then.
//!
//! Phase 6 enables the feature (see `src-tauri`'s `analytics` feature) and
//! creates `analytics.duckdb` with the `corpus_chunks` table that Phase 7
//! fills. The store never writes to SQLite: results travel back through the
//! Rust repository layer only.

pub use el_core::{AppError, AppResult};

/// Name of the on-disk analytical file created in Phase 6.
pub const ANALYTICS_DB_FILENAME: &str = "analytics.duckdb";

/// Name of the chunk table defined in Section 5 of the implementation plan.
pub const CORPUS_CHUNKS_TABLE: &str = "corpus_chunks";

#[cfg(feature = "duckdb")]
mod engine;

#[cfg(feature = "duckdb")]
pub use engine::{Analytics, Chunk};
