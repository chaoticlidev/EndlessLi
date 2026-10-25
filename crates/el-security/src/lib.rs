//! `el-security`: secrets, sanitization, and encryption.
//!
//! API keys live in the OS keychain and are never returned to the frontend;
//! LLM output is stripped of active HTML/scripts; note and database
//! encryption land here in the release hardening phase.

pub mod crypto;
pub mod sanitize;
pub mod secrets;

pub use el_core::{AppError, AppResult};
pub use sanitize::sanitize_llm_output;
pub use secrets::{delete_api_key, has_api_key, load_api_key, set_api_key};
