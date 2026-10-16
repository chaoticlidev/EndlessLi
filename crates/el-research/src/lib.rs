//! `el-research`: Light Search and Deep Dive (RLM) orchestration.
//!
//! The Python sidecar does search and clipping; the LLM client synthesizes.
//! Rust owns session state, cancellation tokens, step events, citation
//! validation, and persistence, so partial results and cancellation stay
//! testable with stub tools and no network.

pub mod citations;
pub mod context;
pub mod research;
pub mod tools;

pub use citations::{renumber_citations, validate_citations};
pub use context::build_context;
pub use el_core::{AppError, AppResult};
pub use research::{
    cancellable, extract_insights, is_cancelled, llm_call, parse_sub_questions, run_deep,
    run_light, CallBudget, ChannelSink, EventSink, LightOutcome, NullSink, ResearchEvent,
    ResearchEventKind, RunConfig, SourceRef, DEEP_PARALLELISM, DEFAULT_CLIP_COUNT,
    DEFAULT_LLM_CALL_CAP, DEFAULT_MAX_CONTEXT_TOKENS, MAX_DEPTH, MAX_SUB_QUESTIONS,
};
