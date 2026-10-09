//! `el-llm`: provider trait, token counting, and budget enforcement.
//!
//! Why this crate exists: every LLM call (research synthesis, skills, memory
//! suggestions) must be counted, logged to `token_usage`, and gated by daily
//! and monthly budgets (C-9). Centralizing the `LlmProvider` trait here keeps
//! OpenAI-compatible, Anthropic, and DeepSeek adapters interchangeable.

pub use el_core::{AppError, AppResult};
