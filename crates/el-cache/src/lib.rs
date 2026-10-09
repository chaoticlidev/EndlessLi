//! `el-cache`: read-through cache with size-based LRU eviction.
//!
//! Why this crate exists: search results, clipped pages, LLM responses, and
//! embeddings are all keyed by SHA-256 of (type, normalized query, model).
//! One eviction policy (default 1 GB, prune to 90% after each write) serves
//! every client, so Phase 4 implements it once here.

pub use el_core::{AppError, AppResult};

/// Default cache ceiling in bytes (1 GB per the implementation plan).
pub const DEFAULT_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// Eviction prunes to this fraction of the ceiling after each write.
pub const EVICTION_TARGET_FRACTION: f64 = 0.9;
