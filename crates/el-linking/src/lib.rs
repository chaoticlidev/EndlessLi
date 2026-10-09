//! `el-linking`: auto-linking orchestration over sqlite-vec.
//!
//! Why this crate exists: suggesting links needs one write path that stores
//! each pair once (`note_a < note_b`), filters rejections, and applies the
//! similarity threshold (default 0.75). Keeping that logic here guarantees
//! no orphaned half-links regardless of which UI surface triggers it.

pub use el_core::{AppError, AppResult};

/// Default cosine-similarity threshold for link candidates.
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.75;

/// Maximum candidates returned per note.
pub const MAX_CANDIDATES: u32 = 5;
