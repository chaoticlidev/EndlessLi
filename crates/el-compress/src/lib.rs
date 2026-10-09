//! `el-compress`: static-corpus processing orchestration.
//!
//! Why this crate exists: chunking, embedding, K-Means, representative
//! selection, and per-cluster generation form one pipeline (Flow 2). Rust
//! drives the sidecar calls, enforces the token-budget fallback for K, and
//! stages the K drafts for approval before anything reaches `notes`.

pub use el_core::{AppError, AppResult};

/// Fixed RNG seed so clustering is reproducible across runs.
pub const CLUSTER_SEED: u64 = 42;
