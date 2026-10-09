//! `el-research`: Light Search and Deep Dive (RLM) orchestration.
//!
//! Why this crate exists: the Python sidecar does the network and LLM work,
//! but Rust owns session state, cancellation tokens, step events, citation
//! validation, and persistence. That boundary keeps partial results and
//! cancellation semantics testable with a mocked sidecar.

pub use el_core::{AppError, AppResult};

/// Maximum recursion depth for Deep Dive sub-questions (C-8).
pub const MAX_DEPTH: u32 = 2;

/// Default cap on total LLM calls per Deep Dive session.
pub const DEFAULT_LLM_CALL_CAP: u32 = 25;
