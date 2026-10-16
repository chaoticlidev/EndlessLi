//! `el-cache`: read-through cache with size-based LRU eviction.
//!
//! Search results, clipped pages, LLM responses, and embeddings are all
//! keyed by SHA-256 of (type, normalized query, model) and share one eviction
//! policy (default 1 GB, prune to 90% after each write). Clients never touch
//! the tables directly; they go through [`get_or_compute`], so a repeated
//! query makes zero network calls by construction.

use std::future::Future;

pub use el_core::{AppError, AppResult};

/// Default cache ceiling in bytes (1 GB per the implementation plan).
pub const DEFAULT_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// Eviction prunes to this fraction of the ceiling after each write.
pub const EVICTION_TARGET_FRACTION: f64 = 0.9;

/// What to cache: the lookup coordinates plus write metadata.
pub struct CacheInput {
    /// One of `search`, `llm_response`, `webpage`, `embedding`.
    pub entry_type: &'static str,
    /// Raw query; normalized before keying.
    pub query: String,
    /// Model or provider id distinguishing the response space.
    pub model: String,
    /// Seconds until expiry. Falls back to the per-type policy when `None`
    /// (search 24h, webpage 7d, everything else immortal).
    pub ttl_secs: Option<i64>,
}

/// A freshly computed value to store.
pub struct CacheValue {
    pub bytes: Vec<u8>,
    pub source_type: String,
}

/// Read-through lookup shared by every network client.
///
/// On a hit, returns the stored bytes without calling `compute`. On a miss
/// (or an expired entry, which reads as a miss), runs `compute` exactly
/// once, stores the result, and returns it. The two database checkouts are
/// short-lived and never span the `await`, so pooled connections are never
/// held across network calls.
pub async fn get_or_compute<F, Fut>(
    db: &el_db::Database,
    input: CacheInput,
    compute: F,
) -> AppResult<(Vec<u8>, bool)>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = AppResult<CacheValue>>,
{
    let ttl = input
        .ttl_secs
        .or_else(|| el_db::repo::cache::default_ttl(input.entry_type));
    {
        let conn = db.connect()?;
        if let Some(hit) =
            el_db::repo::cache::get(&conn, input.entry_type, &input.query, &input.model)?
        {
            return Ok((hit.response, true));
        }
    }
    let fresh = compute().await?;
    {
        let conn = db.connect()?;
        el_db::repo::cache::put(
            &conn,
            input.entry_type,
            &input.query,
            &input.model,
            &fresh.bytes,
            &fresh.source_type,
            ttl,
        )?;
    }
    Ok((fresh.bytes, false))
}
