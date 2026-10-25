//! You.com MCP search, invoked as a Rust-only fallback when the Python sidecar
//! cannot reach the You.com API (no API key configured, or the API call failed).
//!
//! This provider speaks the You.com MCP JSON-RPC 2.0 surface: a stateless POST
//! to https://api.you.com/mcp?profile=free with a single `search` tool call.
//! It is intentionally minimal — returns normalized hits or an empty vec on any
//! transport-level failure (HTTP 429, timeout, parse error). The caller's
//! fallback chain already covers total failure by moving to the next provider.

use el_core::{AppError, AppResult};
use el_research::tools::SearchHit;
use serde::Deserialize;
use serde_json::json;

/// Searches You.com MCP for one query, returning normalized hits.
///
/// The You.com API key (service name `you`) is loaded from the OS keychain.
/// When no key is stored, this returns an empty vec with no error so the caller
/// can fall through silently to the next provider in the chain.
pub async fn search(query: &str, max_results: u32) -> AppResult<Vec<SearchHit>> {
    let key = match el_security::load_api_key("you") {
        Ok(key) => key,
        Err(el_core::AppError::InvalidInput(_)) => {
            // No You.com key configured — not an error, just nothing to offer.
            return Ok(Vec::new());
        }
        Err(err) => return Err(err),
    };

    let client = new_client()?;
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "search",
        "params": {
            "query": query,
            "num": max_results.min(20),
        },
    });

    let response = client
        .post("https://api.you.com/mcp?profile=free")
        .header("x-api-key", &key)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await;

    let response = match response {
        Ok(resp) => resp,
        Err(err) => {
            tracing::warn!("you.com MCP request failed: {err}");
            return Ok(Vec::new());
        }
    };

    if response.status().as_u16() == 429 {
        tracing::debug!("you.com MCP rate-limited (429); skipping");
        return Ok(Vec::new());
    }

    let payload: serde_json::Value = match response.json().await {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!("you.com MCP response parse failed: {err}");
            return Ok(Vec::new());
        }
    };

    // The MCP response wraps results in a tool "content" field; the You.com
    // MCP profile nests hits under result.results or result.content depending
    // on the profile. We try several known locations.
    let results: Vec<serde_json::Value> = payload
        .get("result")
        .and_then(|result| result.get("results"))
        .or_else(|| payload.get("result").and_then(|result| result.get("content")))
        .or_else(|| payload.get("content"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let mut hits = Vec::new();
    for item in results.iter().take(max_results as usize) {
        let url = item.get("url").and_then(|v| v.as_str()).or_else(|| {
            item.get("link").and_then(|v| v.as_str())
        });
        if url.is_none() || url.unwrap().is_empty() {
            continue;
        }
        let parsed: YouResult = match serde_json::from_value(item.clone()) {
            Ok(parsed) => parsed,
            Err(_) => {
                hits.push(SearchHit {
                    title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    url: url.unwrap().to_string(),
                    snippet: item.get("snippet").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
                continue;
            }
        };
        hits.push(SearchHit {
            title: parsed.title.unwrap_or_default(),
            url: parsed.url.unwrap_or_else(|| parsed.link.unwrap_or_default()),
            snippet: parsed.snippet.unwrap_or_default(),
        });
    }

    Ok(hits)
}

/// One You.com MCP result entry; fields are best-effort.
#[derive(Debug, Deserialize)]
struct YouResult {
    title: Option<String>,
    url: Option<String>,
    link: Option<String>,
    snippet: Option<String>,
}

fn new_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| AppError::Internal(format!("build You.com MCP client: {err}")))
}
