//! Research commands: run management over the el-research orchestrator.
//!
//! Handlers build providers and run configuration from settings, spawn the
//! orchestration as background tasks, and stream progress through typed
//! Tauri events. Persistence and citation rules live in `el-research`; the
//! sidecar tools below translate its tool trait into cached RPC calls.

use std::sync::Arc;

use el_core::{
    AppError, AppResult, NewNote, NewSession, Note, ResearchResult, ResearchSession, ResearchStep,
};
use el_llm::LlmProvider;
use el_research::research::{is_cancelled, ChannelSink, RunConfig};
use el_research::tools::{ClipDoc, ResearchTools, SearchHit};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// Maximum concurrent sidecar RPC calls to avoid overwhelming the Python
/// process during Deep Dive parallelism.
const SIDECAR_CONCURRENCY: usize = 2;

/// Progress on one research step, fanned out as `research://step`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "research://step")]
pub struct ResearchStepEvent {
    pub session_id: String,
    pub step: ResearchStep,
}

/// A stored research result, fanned out as `research://result`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "research://result")]
pub struct ResearchResultEvent {
    pub session_id: String,
    pub result: ResearchResult,
}

/// Session lifecycle changes, fanned out as `research://status`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "research://status")]
pub struct ResearchStatusEvent {
    pub session_id: String,
    pub status: String,
}

/// Full session state for the dashboard run view.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ResearchView {
    pub session: ResearchSession,
    pub steps: Vec<ResearchStep>,
    pub results: Vec<ResearchResult>,
}

/// Translates the tool trait into cached sidecar RPC calls.
#[derive(Debug, Clone)]
struct SidecarTools {
    db: el_db::Database,
    sidecar: crate::sidecar::SidecarManager,
    /// Limits concurrent sidecar RPC calls to 2 so the Python process
    /// never gets overwhelmed during Deep Dive parallelism.
    concurrency: Arc<Semaphore>,
}

impl ResearchTools for SidecarTools {
    async fn search(
        &self,
        query: &str,
        max_results: u32,
        providers: &[String],
        searxng_url: Option<&str>,
        brave_key: Option<&str>,
        tavily_key: Option<&str>,
        exa_key: Option<&str>,
        keenable_key: Option<&str>,
        firecrawl_key: Option<&str>,
        bing_key: Option<&str>,
        parallel_key: Option<&str>,
        anysearch_key: Option<&str>,
        perplexity_key: Option<&str>,
        serpbase_key: Option<&str>,
        you_key: Option<&str>,
        safe_search: &str,
    ) -> AppResult<(Vec<SearchHit>, String)> {
        let model = providers.join("+");
        let (bytes, _) = el_cache::get_or_compute(
            &self.db,
            el_cache::CacheInput {
                entry_type: "search",
                query: query.to_string(),
                model,
                ttl_secs: None,
            },
            || async {
                let _permit = self
                    .concurrency
                    .acquire()
                    .await
                    .map_err(|_| AppError::Internal("sidecar semaphore closed".to_string()))?;
                let sidecar_result = self.sidecar.call(
                    "search.query",
                    serde_json::json!({
                        "query": query,
                        "max_results": max_results,
                        "providers": providers,
                        "searxng_url": searxng_url.unwrap_or(""),
                        "brave_key": brave_key.unwrap_or(""),
                        "tavily_key": tavily_key.unwrap_or(""),
                        "exa_key": exa_key.unwrap_or(""),
                        "keenable_key": keenable_key.unwrap_or(""),
                        "firecrawl_key": firecrawl_key.unwrap_or(""),
                        "bing_key": bing_key.unwrap_or(""),
                        "parallel_key": parallel_key.unwrap_or(""),
                        "anysearch_key": anysearch_key.unwrap_or(""),
                        "perplexity_key": perplexity_key.unwrap_or(""),
                        "serpbase_key": serpbase_key.unwrap_or(""),
                        "you_key": you_key.unwrap_or(""),
                        "safe_search": safe_search,
                    }),
                ).await;
                let response = match sidecar_result {
                    Ok(resp) => resp,
                    Err(err) => {
                        tracing::warn!(error = %err, "sidecar search failed; trying You.com MCP fallback");
                        if you_key.is_some() {
                            let mcp_hits = crate::commands::you_mcp::search(query, max_results).await.unwrap_or_default();
                            if !mcp_hits.is_empty() {
                                return Ok(el_cache::CacheValue {
                                    bytes: serde_json::json!({"provider": "you", "results": []})
                                        .to_string()
                                        .into_bytes(),
                                    source_type: "you".to_string(),
                                });
                            }
                        }
                        return Err(err);
                    }
                };
                let provider = response
                    .get("provider")
                    .and_then(|provider| provider.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let mut hits: Vec<SearchHit> = Vec::new();
                if let Some(results) = response
                    .get("results")
                    .and_then(|results| results.as_array())
                {
                    for result in results {
                        let url = result.get("url").and_then(|url| url.as_str()).unwrap_or("");
                        if url.is_empty() {
                            continue;
                        }
                        hits.push(SearchHit {
                            title: result
                                .get("title")
                                .and_then(|title| title.as_str())
                                .unwrap_or("")
                                .to_string(),
                            url: url.to_string(),
                            snippet: result
                                .get("snippet")
                                .and_then(|snippet| snippet.as_str())
                                .unwrap_or("")
                                .to_string(),
                        });
                    }
                }
                // If the sidecar returned no hits but You.com MCP is
                // available, try that as a last-resort fallback before
                // giving up on the whole chain.
                if hits.is_empty() && you_key.is_some() {
                    let mcp_hits = crate::commands::you_mcp::search(query, max_results).await.unwrap_or_default();
                    if !mcp_hits.is_empty() {
                        hits = mcp_hits;
                    }
                }
                Ok(el_cache::CacheValue {
                    bytes: serde_json::json!({"provider": provider, "results": hits})
                        .to_string()
                        .into_bytes(),
                    source_type: provider,
                })
            },
        )
        .await?;
        let response: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|err| AppError::Internal(format!("decode cached search: {err}")))?;
        let provider = response
            .get("provider")
            .and_then(|provider| provider.as_str())
            .unwrap_or("unknown")
            .to_string();
        let mut hits = Vec::new();
        if let Some(results) = response
            .get("results")
            .and_then(|results| results.as_array())
        {
            for result in results {
                let url = result.get("url").and_then(|url| url.as_str()).unwrap_or("");
                if url.is_empty() {
                    continue;
                }
                hits.push(SearchHit {
                    title: result
                        .get("title")
                        .and_then(|title| title.as_str())
                        .unwrap_or("")
                        .to_string(),
                    url: url.to_string(),
                    snippet: result
                        .get("snippet")
                        .and_then(|snippet| snippet.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
        Ok((hits, provider))
    }

    async fn clip(&self, url: &str, max_chars: u32) -> AppResult<ClipDoc> {
        let (bytes, _) = el_cache::get_or_compute(
            &self.db,
            el_cache::CacheInput {
                entry_type: "webpage",
                query: url.to_string(),
                model: "readability".to_string(),
                ttl_secs: None,
            },
            || async {
                let _permit = self
                    .concurrency
                    .acquire()
                    .await
                    .map_err(|_| AppError::Internal("sidecar semaphore closed".to_string()))?;
                let response = self
                    .sidecar
                    .call(
                        "clip.url",
                        serde_json::json!({ "url": url, "max_chars": max_chars }),
                    )
                    .await?;
                Ok(el_cache::CacheValue {
                    bytes: serde_json::to_vec(&response).map_err(|err| {
                        AppError::Internal(format!("encode clip response: {err}"))
                    })?,
                    source_type: "clip".to_string(),
                })
            },
        )
        .await?;
        let response: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|err| AppError::Internal(format!("decode cached clip: {err}")))?;
        Ok(ClipDoc {
            url: response
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or(url)
                .to_string(),
            title: response
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            text: response
                .get("text")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            truncated: response
                .get("truncated")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        })
    }
}

/// Starts a Light or Deep run and returns the session id immediately.
/// Progress streams through `research://step`, `research://result`, and
/// `research://status` events.
#[tauri::command]
#[specta::specta]
pub async fn research_start(
    state: State<'_, AppState>,
    query: String,
    mode: String,
    project_id: Option<String>,
    folder_id: Option<String>,
    session_memory: Option<String>,
) -> AppResult<String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err(AppError::InvalidInput(
            "query must not be blank".to_string(),
        ));
    }
    if query.chars().count() > 8_000 {
        return Err(AppError::InvalidInput(
            "query exceeds the 8,000-character limit".to_string(),
        ));
    }
    if let Some(memory) = &session_memory {
        if memory.chars().count() > 50_000 {
            return Err(AppError::InvalidInput(
                "session memory exceeds the 50,000-character limit".to_string(),
            ));
        }
    }
    if mode != "light" && mode != "deep" {
        return Err(AppError::InvalidInput(
            "mode must be light or deep".to_string(),
        ));
    }
    // Resolve everything that can fail BEFORE creating the session: a
    // validation error here must not leave an orphaned in-progress session
    // with no task behind it (it would show as running forever).
    let config = run_config(
        &state,
        project_id.clone(),
        folder_id.clone(),
        session_memory.clone(),
    )?;
    let provider = crate::commands::provider::build_provider(&state)?;
    let model = crate::commands::provider::configured_model(&state)?;
    log_research_setup(provider.name(), &model);

    let conn = state.db.connect()?;
    let session = el_db::repo::research::create_session(
        &conn,
        &NewSession {
            query: query.clone(),
            mode: mode.clone(),
            project_id: project_id.clone(),
            folder_id: folder_id.clone(),
            context_memory: session_memory.clone(),
        },
    )?;
    el_db::repo::research::set_session_status(&conn, &session.id, "in_progress")?;
    drop(conn);

    let session_id = session.id.clone();
    let calls = Arc::new(el_research::research::CallBudget::new(config.llm_call_cap));
    let cancel = CancellationToken::new();
    state.research.insert(session_id.clone(), cancel.clone());

    let task_state = state.inner().clone();
    let task_session = session_id.clone();
    let task_query = query.clone();
    let task_mode = mode.clone();
    tauri::async_runtime::spawn(async move {
        let tools = Arc::new(SidecarTools {
            db: task_state.db.clone(),
            sidecar: task_state.sidecar.clone(),
            concurrency: Arc::new(Semaphore::new(SIDECAR_CONCURRENCY)),
        });
        let sink = Arc::new(ChannelSink::new(task_state.research_tx.clone()));
        let outcome = if task_mode == "deep" {
            el_research::research::run_deep(
                task_state.db.clone(),
                tools,
                sink,
                cancel,
                Arc::new(provider),
                model,
                task_session.clone(),
                task_query,
                config,
                calls,
            )
            .await
            .map(|_| ())
        } else {
            el_research::research::run_light(
                task_state.db.clone(),
                tools,
                sink,
                cancel,
                Arc::new(provider),
                model,
                task_session.clone(),
                None,
                task_query,
                config,
                calls,
            )
            .await
            .map(|_| ())
        };
        finish_run(&task_state, &task_session, outcome);
    });

    emit_status(&state, &session_id, "in_progress");
    Ok(session_id)
}

/// One-line diagnostics for research setup: provider id and whether a model
/// and key are configured. Never logs key material or full settings.
fn log_research_setup(provider: &str, model: &str) {
    let key_present = el_security::has_api_key(provider).unwrap_or(false);
    tracing::info!(
        provider = provider,
        model_set = !model.trim().is_empty(),
        key_present = key_present,
        "research start"
    );
}

/// Builds the run configuration from settings plus the keychain.
fn run_config(
    state: &AppState,
    project_id: Option<String>,
    folder_id: Option<String>,
    session_memory: Option<String>,
) -> AppResult<RunConfig> {
    let conn = state.db.connect()?;
    let get = |key: &str| el_db::repo::cache::get_setting(&conn, key);
    let providers: Vec<String> = match get("research.providers")? {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| vec!["ddgs".to_string()]),
        None => vec!["ddgs".to_string()],
    };
    let safe_search = get("research.safe_search")?.unwrap_or_else(|| "off".to_string());
    if safe_search != "off" && safe_search != "strict" && safe_search != "moderate" {
        tracing::warn!(safe_search, "invalid safe_search value; defaulting to off");
    }
    let safe_level = if safe_search == "strict" || safe_search == "moderate" {
        safe_search.as_str()
    } else {
        "off"
    };
    let brave_key = el_security::load_api_key("brave").ok();
    let tavily_key = el_security::load_api_key("tavily").ok();
    let exa_key = el_security::load_api_key("exa").ok();
    let keenable_key = el_security::load_api_key("keenable").ok();
    let firecrawl_key = el_security::load_api_key("firecrawl").ok();
    let bing_key = el_security::load_api_key("bing").ok();
    let parallel_key = el_security::load_api_key("parallel").ok();
    let anysearch_key = el_security::load_api_key("anysearch").ok();
    let perplexity_key = el_security::load_api_key("perplexity").ok();
    let serpbase_key = el_security::load_api_key("serpbase").ok();
    let you_key = el_security::load_api_key("you").ok();
    Ok(RunConfig {
        providers,
        searxng_url: get("research.searxng_url")?.filter(|url| !url.trim().is_empty()),
        brave_key,
        tavily_key,
        exa_key,
        keenable_key,
        firecrawl_key,
        bing_key,
        parallel_key,
        anysearch_key,
        perplexity_key,
        serpbase_key,
        you_key,
        safe_search: safe_level.to_string(),
        clip_count: get("research.clip_count")?
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or(el_research::DEFAULT_CLIP_COUNT),
        max_context_tokens: get("research.max_context_tokens")?
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or(el_research::DEFAULT_MAX_CONTEXT_TOKENS),
        llm_call_cap: get("research.llm_call_cap")?
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or(el_research::DEFAULT_LLM_CALL_CAP),
        project_id,
        folder_id,
        session_memory,
    })
}

/// Persists the terminal status, emits it, and forgets the run.
fn finish_run(state: &AppState, session_id: &str, outcome: AppResult<()>) {
    let status = match &outcome {
        Ok(()) => "completed",
        Err(err) if is_cancelled(err) => "cancelled",
        Err(_) => "failed",
    };
    if let Ok(conn) = state.db.connect() {
        let _ = el_db::repo::research::set_session_status(&conn, session_id, status);
    }
    state.research.remove(session_id);
    emit_status(state, session_id, status);
}

fn emit_status(state: &AppState, session_id: &str, status: &str) {
    state
        .research_tx
        .send(el_research::ResearchEvent {
            session_id: session_id.to_string(),
            kind: el_research::ResearchEventKind::Status(status.to_string()),
        })
        .ok();
}

/// Stops a running session. Partial steps and results are kept.
#[tauri::command]
#[specta::specta]
pub fn research_cancel(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.research.cancel(&session_id);
    Ok(())
}

/// Deletes a session, its steps, and its results.
///
/// Saved notes are preserved: the schema clears `notes.source_session_id`
/// (`ON DELETE SET NULL`) instead of deleting the row. A run still in flight
/// is cancelled first and winds down at its next checkpoint.
#[tauri::command]
#[specta::specta]
pub fn research_delete(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    if state.research.cancel(&session_id) {
        tracing::info!(session_id = %session_id, "cancelled run while deleting session");
    }
    let conn = state.db.connect()?;
    el_db::repo::research::delete_session(&conn, &session_id)
}

/// Fetches full session state for the dashboard run view.
///
/// A session left `pending`/`in_progress` with no tracked run (crashed app,
/// or a start that failed before spawning) is marked `failed` on read so it
/// can never display as running forever.
#[tauri::command]
#[specta::specta]
pub fn research_get(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<Option<ResearchView>> {
    let conn = state.db.connect()?;
    let Some(session) = el_db::repo::research::get_session(&conn, &session_id)? else {
        return Ok(None);
    };
    let session = if (session.status == "pending" || session.status == "in_progress")
        && !state.research.contains(&session_id)
    {
        el_db::repo::research::set_session_status(&conn, &session_id, "failed")?
    } else {
        session
    };
    Ok(Some(ResearchView {
        steps: el_db::repo::research::list_steps(&conn, &session_id)?,
        results: el_db::repo::research::list_results(&conn, &session_id)?,
        session,
    }))
}

/// Lists sessions newest-first for the history panel.
#[tauri::command]
#[specta::specta]
pub fn research_history(state: State<'_, AppState>, limit: u32) -> AppResult<Vec<ResearchSession>> {
    let conn = state.db.connect()?;
    el_db::repo::research::list_sessions(&conn, None, limit as usize)
}

/// Marks results for saving, then saves them as one AI-authored note.
/// The note content carries the source list, so every saved note traces
/// back to its URLs.
#[tauri::command]
#[specta::specta]
pub fn research_save_results(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
    project_id: Option<String>,
    folder_id: Option<String>,
    result_ids: Vec<String>,
) -> AppResult<Note> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::InvalidInput(
            "title must not be blank".to_string(),
        ));
    }
    if result_ids.is_empty() {
        return Err(AppError::InvalidInput(
            "select at least one result".to_string(),
        ));
    }
    let conn = state.db.connect()?;
    let session = el_db::repo::research::require_session(&conn, &session_id)?;
    let all = el_db::repo::research::list_results(&conn, &session_id)?;
    let chosen: Vec<_> = all
        .iter()
        .filter(|result| result_ids.contains(&result.id))
        .collect();
    if chosen.is_empty() {
        return Err(AppError::InvalidInput(
            "none of the selected results belong to this session".to_string(),
        ));
    }
    let mut content = String::new();
    for result in &chosen {
        if result.result_type == "summary" || result.result_type == "insight" {
            content.push_str(&result.content);
            content.push_str("\n\n");
        }
    }
    let sources: Vec<_> = all
        .iter()
        .filter(|result| result.result_type == "source")
        .collect();
    if !sources.is_empty() {
        content.push_str("## Sources\n\n");
        for source in sources {
            let label = source.source_title.as_deref().unwrap_or("Source");
            match source.source_url.as_deref() {
                Some(url) => content.push_str(&format!("- [{label}]({url})\n")),
                None => content.push_str(&format!("- {label}\n")),
            }
        }
    }
    content.push_str(&format!(
        "\n*Researched for {:?}; review before sharing.*\n",
        session.query
    ));
    let note = el_db::repo::notes::create(
        &conn,
        &NewNote {
            title,
            content: content.trim().to_string(),
            project_id,
            folder_id,
            author: "ai".to_string(),
            source_session_id: Some(session_id.clone()),
            created_from_note_id: None,
        },
    )?;
    el_db::repo::research::attach_note(&conn, &session_id, &note.id)?;
    for result in chosen {
        let _ = el_db::repo::research::set_result_selected(&conn, &result.id, true);
    }
    drop(conn);
    state.embed.enqueue(note.id.clone());
    Ok(note)
}

/// Reports sidecar lifecycle state for diagnostics.
#[tauri::command]
#[specta::specta]
pub fn sidecar_status(state: State<'_, AppState>) -> AppResult<String> {
    Ok(format!("{:?}", state.sidecar.status()))
}

#[tauri::command]
#[specta::specta]
pub async fn research_rename(
    state: tauri::State<'_, AppState>,
    session_id: String,
    new_title: String,
) -> Result<(), AppError> {
    let conn = state.db.connect()?;
    el_db::repo::research::update_query(&conn, &session_id, &new_title)?;
    Ok(())
}
