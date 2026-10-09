//! `el-skills`: two-tier skills (Markdown) and scripts (Python).
//!
//! Why this crate exists: skills render a prompt template through the LLM
//! while scripts run in a sandboxed child process (C-3, C-4). Parsing,
//! parameter validation, and the approval gate live here so both tiers share
//! one execution contract.

pub use el_core::{AppError, AppResult};

/// Default script timeout in seconds.
pub const DEFAULT_SCRIPT_TIMEOUT_SECS: u64 = 60;
