//! `el-memory`: global / project / session memory hierarchy.
//!
//! Why this crate exists: memory is user-controlled (ADR-008). AI suggestions
//! land in `memory_suggestions` and only become rows in `memory_entries` after
//! approval. Prompt assembly (session, then project, then global) lives here
//! so every caller gets identical precedence and dedup behavior.

pub use el_core::{AppError, AppResult};
