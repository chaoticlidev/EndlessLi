//! `el-analytics`: DuckDB analytical store for Semantic Compression.
//!
//! Why this crate exists: SQLite is the transactional source of truth, but
//! K-Means input preparation and large aggregations belong in a columnar
//! engine. This crate stays behind the `duckdb` Cargo feature until Phase 6
//! so CI and local builds pay no compile-time cost before then.

pub use el_core::{AppError, AppResult};

/// Name of the on-disk analytical file created in Phase 6.
pub const ANALYTICS_DB_FILENAME: &str = "analytics.duckdb";

/// Name of the chunk table defined in Section 5 of the implementation plan.
pub const CORPUS_CHUNKS_TABLE: &str = "corpus_chunks";
