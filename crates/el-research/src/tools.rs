//! Tool boundary between orchestration and the Python sidecar.
//!
//! [`ResearchTools`] abstracts search and clipping so orchestration is
//! unit-testable with stubs and the Tauri shell implements it over the
//! sidecar (with read-through caching). Methods take plain data and return
//! plain data; transport, caching, and key handling live in the shell.

use el_core::AppResult;
use serde::Serialize;
use std::future::Future;

/// One normalized search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// One clipped page with provenance for citation.
#[derive(Debug, Clone, Serialize)]
pub struct ClipDoc {
    pub url: String,
    pub title: String,
    pub text: String,
    pub truncated: bool,
}

/// Search and clipping capabilities used by Light and Deep runs.
///
/// Methods are desugared futures with explicit `Send` bounds so Deep
/// children can spawn onto tasks.
pub trait ResearchTools: Send + Sync {
    /// Runs a web search across the requested providers with fallback.
    /// Returns hits plus the provider that served them (for cache labels)
    #[allow(clippy::too_many_arguments)]
    fn search<'a>(
        &'a self,
        query: &'a str,
        max_results: u32,
        providers: &'a [String],
        searxng_url: Option<&'a str>,
        brave_key: Option<&'a str>,
        tavily_key: Option<&'a str>,
        exa_key: Option<&'a str>,
        keenable_key: Option<&'a str>,
        firecrawl_key: Option<&'a str>,
        bing_key: Option<&'a str>,
        parallel_key: Option<&'a str>,
        anysearch_key: Option<&'a str>,
        perplexity_key: Option<&'a str>,
        serpbase_key: Option<&'a str>,
        you_key: Option<&'a str>,
        safe_search: &'a str,
    ) -> impl Future<Output = AppResult<(Vec<SearchHit>, String)>> + Send + 'a;

    /// Clips readable text from one HTTPS page.
    fn clip<'a>(
        &'a self,
        url: &'a str,
        max_chars: u32,
    ) -> impl Future<Output = AppResult<ClipDoc>> + Send + 'a;
}
