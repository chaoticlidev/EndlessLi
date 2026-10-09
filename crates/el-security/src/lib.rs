//! `el-security`: secrets, sanitization, and encryption.
//!
//! Why this crate exists: API keys live in the OS keychain and are never
//! returned to the frontend; LLM output is stripped of active HTML/scripts;
//! note and database encryption land here in Phase 11. One crate makes the
//! trust boundaries auditable in a single place.

pub use el_core::{AppError, AppResult};
