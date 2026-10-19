//! Light Search and Deep Dive (RLM) orchestration.
//!
//! The Rust side owns session state, cancellation, citation validation, and
//! persistence; the Python sidecar provides search and clipping tools; the
//! LLM client provides synthesis. Light mode is one pass (search, clip,
//! synthesize). Deep mode plans sub-questions, runs each as a Light pass in
//! a child step (at most two at once), then merges bottom-up with global
//! citation numbers. Partial steps and results persist throughout, so
//! cancellation and failures always leave something behind.
//!
//! Everything takes owned values and `Arc` handles so Deep children can run
//! as spawned tasks: nothing borrowed crosses an awaited task boundary.

use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use el_core::{AppError, AppResult, NewResult, NewStep};
use el_llm::{ChatMessage, ChatRequest, LlmProvider};
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::citations::{renumber_citations, validate_citations};
use crate::context::build_context;
use crate::tools::ResearchTools;

/// Maximum sub-questions per Deep plan.
pub const MAX_SUB_QUESTIONS: usize = 5;

/// Maximum parallel child runs in Deep mode.
pub const DEEP_PARALLELISM: usize = 2;

/// Default cap on total LLM calls per session.
pub const DEFAULT_LLM_CALL_CAP: u32 = 25;

/// Maximum recursion depth: Deep plans run at level 0 and their children run
/// Light passes at level 1, which never plan again. Level 2 is terminal by
/// construction, so the bound holds without threading a counter.
pub const MAX_DEPTH: u32 = 2;

/// Default number of pages clipped per Light pass.
pub const DEFAULT_CLIP_COUNT: usize = 5;

/// Default token budget for clipped context per synthesis call.
pub const DEFAULT_MAX_CONTEXT_TOKENS: usize = 6000;

/// Orchestration progress pushed to the shell for Tauri events.
#[derive(Debug, Clone)]
pub struct ResearchEvent {
    pub session_id: String,
    pub kind: ResearchEventKind,
}

/// Step created/updated, result stored, or session status changed.
#[derive(Debug, Clone)]
pub enum ResearchEventKind {
    Step(el_core::ResearchStep),
    Result(el_core::ResearchResult),
    Status(String),
}

/// Receives orchestration progress. The shell forwards to Tauri events.
pub trait EventSink: Send + Sync {
    /// Emits one event. Implementations must not block orchestration.
    fn emit(&self, event: ResearchEvent);
}

/// A sink that drops events when nobody listens.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: ResearchEvent) {}
}

/// A sink forwarding into an async channel (one per run in the shell).
pub struct ChannelSink {
    tx: mpsc::UnboundedSender<ResearchEvent>,
}

impl ChannelSink {
    /// Creates a sink around the channel sender.
    pub fn new(tx: mpsc::UnboundedSender<ResearchEvent>) -> Self {
        Self { tx }
    }
}

impl EventSink for ChannelSink {
    fn emit(&self, event: ResearchEvent) {
        let _ = self.tx.send(event);
    }
}

/// Shared knobs for one run, read from settings by the shell.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Ordered provider ids with fallback, e.g. `["ddgs", "searxng"]`.
    pub providers: Vec<String>,
    /// SearXNG base URL for the `searxng` provider.
    pub searxng_url: Option<String>,
    /// Brave token for the `brave` provider.
    pub brave_key: Option<String>,
    /// Pages clipped per Light pass.
    pub clip_count: usize,
    /// Token budget for clipped context per synthesis call.
    pub max_context_tokens: usize,
    /// Cap on total LLM calls for the session.
    pub llm_call_cap: u32,
    /// Project scope for memory and saved notes.
    pub project_id: Option<String>,
    /// Folder scope for saved notes.
    pub folder_id: Option<String>,
    /// Temporary session memory for this run.
    pub session_memory: Option<String>,
}

/// Tracks LLM call spending for one session. Sharing across tasks is safe.
#[derive(Debug, Default)]
pub struct CallBudget {
    used: AtomicU32,
    cap: AtomicU32,
}

impl CallBudget {
    /// Creates a counter capped at `cap` calls.
    pub fn new(cap: u32) -> Self {
        Self {
            used: AtomicU32::new(0),
            cap: AtomicU32::new(cap),
        }
    }

    /// Reserves one call, failing when the cap is reached.
    pub fn take(&self) -> AppResult<()> {
        let used = self.used.fetch_add(1, Ordering::SeqCst) + 1;
        let cap = self.cap.load(Ordering::SeqCst);
        if used > cap {
            return Err(AppError::InvalidInput(format!(
                "LLM call cap reached ({used}/{cap})"
            )));
        }
        Ok(())
    }
}

/// One gated, counted, logged, sanitized model call.
pub async fn llm_call<P: LlmProvider>(
    db: &el_db::Database,
    provider: &P,
    model: &str,
    session_id: &str,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    calls: &CallBudget,
) -> AppResult<String> {
    calls.take()?;
    let estimate = el_llm::estimate_messages(
        model,
        &messages
            .iter()
            .map(|message| (message.role.clone(), message.content.clone()))
            .collect::<Vec<_>>(),
    );
    {
        let conn = db.connect()?;
        el_llm::check_budget(&conn, estimate.tokens as i64)?;
    }
    let request = ChatRequest {
        model: model.to_string(),
        messages,
        max_tokens,
        temperature: Some(0.2),
    };
    let response = provider.complete(request).await?;
    let text = el_security::sanitize_llm_output(&response.text);
    {
        let mut conn = db.connect()?;
        el_db::repo::research::add_tokens(
            &mut conn,
            session_id,
            provider.name(),
            model,
            response.usage.tokens_in,
            response.usage.tokens_out,
        )?;
    }
    Ok(text)
}

fn check_cancelled(cancel: &CancellationToken, session_id: &str) -> AppResult<()> {
    if cancel.is_cancelled() {
        return Err(AppError::Internal(format!("cancelled:{session_id}")));
    }
    Ok(())
}

/// Awaits `future` but returns a cancellation error as soon as `cancel`
/// fires, abandoning the inner work. This is what makes Stop responsive
/// during long network or model calls instead of only between steps.
pub async fn cancellable<T>(
    cancel: &CancellationToken,
    session_id: &str,
    future: impl Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(AppError::Internal(format!("cancelled:{session_id}"))),
        result = future => result,
    }
}

/// Reports whether an error is run cancellation (not a real failure).
pub fn is_cancelled(err: &AppError) -> bool {
    matches!(err, AppError::Internal(message) if message.starts_with("cancelled:"))
}

fn record_step(
    db: &el_db::Database,
    sink: &dyn EventSink,
    session_id: &str,
    parent: Option<&str>,
    description: &str,
    status: &str,
) -> AppResult<el_core::ResearchStep> {
    let conn = db.connect()?;
    let step = el_db::repo::research::add_step(
        &conn,
        &NewStep {
            session_id: session_id.to_string(),
            parent_step_id: parent.map(str::to_string),
            description: description.to_string(),
            status: status.to_string(),
            details: None,
        },
    )?;
    sink.emit(ResearchEvent {
        session_id: session_id.to_string(),
        kind: ResearchEventKind::Step(step.clone()),
    });
    Ok(step)
}

fn finish_step(
    db: &el_db::Database,
    sink: &dyn EventSink,
    session_id: &str,
    step_id: &str,
    status: &str,
) {
    if let Ok(conn) = db.connect() {
        let _ = el_db::repo::research::set_step_status(&conn, step_id, status);
        if let Ok(steps) = el_db::repo::research::list_steps(&conn, session_id) {
            if let Some(step) = steps.into_iter().find(|step| step.id == step_id) {
                sink.emit(ResearchEvent {
                    session_id: session_id.to_string(),
                    kind: ResearchEventKind::Step(step),
                });
            }
        }
    }
}

fn store_result(
    db: &el_db::Database,
    sink: &dyn EventSink,
    session_id: &str,
    result_type: &str,
    content: &str,
    source_url: Option<&str>,
    source_title: Option<&str>,
) -> AppResult<el_core::ResearchResult> {
    let conn = db.connect()?;
    let result = el_db::repo::research::add_result(
        &conn,
        &NewResult {
            session_id: session_id.to_string(),
            result_type: result_type.to_string(),
            content: content.to_string(),
            source_url: source_url.map(str::to_string),
            source_title: source_title.map(str::to_string),
        },
    )?;
    sink.emit(ResearchEvent {
        session_id: session_id.to_string(),
        kind: ResearchEventKind::Result(result.clone()),
    });
    Ok(result)
}

/// A clipped source with its citation number for one synthesis call.
struct NumberedSource {
    number: usize,
    url: String,
    title: String,
    text: String,
}

/// A persisted source reference, in citation order.
#[derive(Debug, Clone)]
pub struct SourceRef {
    pub url: String,
    pub title: String,
}

/// A finished Light pass: validated summary plus ordered source references.
#[derive(Debug, Clone)]
pub struct LightOutcome {
    pub summary: String,
    pub sources: Vec<SourceRef>,
}

fn synthesis_prompt(query: &str, memory: &str, sources: &[NumberedSource]) -> String {
    let mut prompt = format!(
        "You are a careful research assistant. Answer the query using ONLY the sources below. \
         Cite every factual claim with a numbered marker like [1] referring to the source list. \
         If the sources do not support an answer, say so plainly instead of guessing.\n\nQuery: {query}\n"
    );
    if !memory.trim().is_empty() {
        prompt.push_str(&format!("\nContext to consider:\n{memory}\n"));
    }
    prompt.push_str("\nSources:\n");
    for source in sources {
        prompt.push_str(&format!(
            "\n[{}] {} — {}\n{}\n",
            source.number, source.title, source.url, source.text
        ));
    }
    prompt
}

/// Drops trailing sources until the prompt fits the token budget.
/// Always keeps at least one source.
fn fit_sources(
    model: &str,
    query: &str,
    memory: &str,
    sources: &mut Vec<NumberedSource>,
    budget: usize,
) {
    while sources.len() > 1 {
        let prompt = synthesis_prompt(query, memory, sources);
        let estimate = el_llm::estimate_text(model, &prompt).tokens;
        if estimate <= budget {
            return;
        }
        sources.pop();
    }
}

/// Searches, clips, and synthesizes one query inside an existing session.
/// Used directly for Light runs and per sub-question in Deep runs.
#[allow(clippy::too_many_arguments)]
pub async fn run_light<P, T, S>(
    db: el_db::Database,
    tools: Arc<T>,
    sink: Arc<S>,
    cancel: CancellationToken,
    provider: Arc<P>,
    model: String,
    session_id: String,
    parent_step: Option<String>,
    query: String,
    config: RunConfig,
    calls: Arc<CallBudget>,
) -> AppResult<LightOutcome>
where
    P: LlmProvider,
    T: ResearchTools,
    S: EventSink,
{
    check_cancelled(&cancel, &session_id)?;
    let search_step = record_step(
        &db,
        &*sink,
        &session_id,
        parent_step.as_deref(),
        &format!("Search: {query}"),
        "in_progress",
    )?;
    let (hits, served_by) = cancellable(
        &cancel,
        &session_id,
        tools.search(
            &query,
            config.clip_count as u32 * 2,
            &config.providers,
            config.searxng_url.as_deref(),
            config.brave_key.as_deref(),
        ),
    )
    .await
    .inspect_err(|_| {
        finish_step(&db, &*sink, &session_id, &search_step.id, "failed");
    })?;
    // The backend that served the search is diagnostic only; it is logged
    // rather than shown to the user, who should not have to care which
    // provider answered (and it would be wrong once fallbacks kick in).
    tracing::debug!(provider = %served_by, query = %query, "search served");
    if hits.is_empty() {
        finish_step(&db, &*sink, &session_id, &search_step.id, "failed");
        return Err(AppError::Internal(format!(
            "search returned no results for {query:?}"
        )));
    }
    finish_step(&db, &*sink, &session_id, &search_step.id, "completed");
    check_cancelled(&cancel, &session_id)?;

    let clip_step = record_step(
        &db,
        &*sink,
        &session_id,
        parent_step.as_deref(),
        &format!("Clip top {} pages", config.clip_count),
        "in_progress",
    )?;
    let mut sources = Vec::new();
    for hit in hits.iter().take(config.clip_count) {
        match cancellable(&cancel, &session_id, tools.clip(&hit.url, 8000)).await {
            Ok(doc) => sources.push(NumberedSource {
                number: 0,
                url: doc.url.clone(),
                title: if doc.title.is_empty() {
                    hit.title.clone()
                } else {
                    doc.title.clone()
                },
                text: doc.text.clone(),
            }),
            Err(err) if is_cancelled(&err) => {
                finish_step(&db, &*sink, &session_id, &clip_step.id, "failed");
                return Err(err);
            }
            Err(err) => {
                tracing::warn!("clip failed for {}: {err}", hit.url);
            }
        }
    }
    // Renumber after skips so markers stay dense.
    for (index, source) in sources.iter_mut().enumerate() {
        source.number = index + 1;
    }
    if sources.is_empty() {
        finish_step(&db, &*sink, &session_id, &clip_step.id, "failed");
        return Err(AppError::Internal(format!("all {query:?} clips failed")));
    }
    finish_step(&db, &*sink, &session_id, &clip_step.id, "completed");
    check_cancelled(&cancel, &session_id)?;

    let synth_step = record_step(
        &db,
        &*sink,
        &session_id,
        parent_step.as_deref(),
        "Synthesize answer",
        "in_progress",
    )?;
    let memory = {
        let conn = db.connect()?;
        build_context(
            &conn,
            config.project_id.as_deref(),
            config.session_memory.as_deref(),
        )?
    };
    fit_sources(
        &model,
        &query,
        &memory,
        &mut sources,
        config.max_context_tokens,
    );
    let prompt = synthesis_prompt(&query, &memory, &sources);
    let raw = cancellable(
        &cancel,
        &session_id,
        llm_call(
            &db,
            &*provider,
            &model,
            &session_id,
            vec![ChatMessage::user(&prompt)],
            Some(1500),
            &calls,
        ),
    )
    .await
    .inspect_err(|_| {
        finish_step(&db, &*sink, &session_id, &synth_step.id, "failed");
    })?;
    let (summary, has_citations) = validate_citations(&raw, sources.len());
    let summary = if has_citations {
        summary
    } else {
        format!("{summary}\n\n(No verifiable citations; treat claims as unverified.)")
    };
    let outcome_sources: Vec<SourceRef> = sources
        .iter()
        .map(|source| {
            let snippet: String = source.text.chars().take(500).collect();
            store_result(
                &db,
                &*sink,
                &session_id,
                "source",
                &snippet,
                Some(&source.url),
                Some(&source.title),
            )
            .map(|_| SourceRef {
                url: source.url.clone(),
                title: source.title.clone(),
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    store_result(&db, &*sink, &session_id, "summary", &summary, None, None)?;
    finish_step(&db, &*sink, &session_id, &synth_step.id, "completed");
    Ok(LightOutcome {
        summary,
        sources: outcome_sources,
    })
}

/// Parses a planner reply into sub-questions, tolerating prose around JSON.
pub fn parse_sub_questions(raw: &str) -> Vec<String> {
    let start = raw.find('{');
    let end = raw.rfind('}');
    let (Some(start), Some(end)) = (start, end) else {
        return Vec::new();
    };
    let candidate = &raw[start..=end];
    let value: serde_json::Value = match serde_json::from_str(candidate) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    value
        .get("sub_questions")
        .and_then(|questions| questions.as_array())
        .map(|questions| {
            questions
                .iter()
                .filter_map(|question| question.as_str())
                .map(str::trim)
                .filter(|question| !question.is_empty())
                .take(MAX_SUB_QUESTIONS)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn planner_prompt(query: &str, memory: &str) -> String {
    format!(
        "You are a research planner. Decompose the query into at most {MAX_SUB_QUESTIONS} \
         focused sub-questions that together cover it. Reply with ONLY this JSON object, \
         no prose: {{\"sub_questions\": [\"...\"]}}\n\nQuery: {query}\n{}",
        if memory.trim().is_empty() {
            String::new()
        } else {
            format!("Context:\n{memory}\n")
        },
    )
}

/// Pulls bullet lines under an insights heading out of a synthesis.
/// Best-effort: returns nothing when the model skipped that section.
pub fn extract_insights(summary: &str) -> Vec<String> {
    let mut insights = Vec::new();
    let mut in_section = false;
    for line in summary.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            in_section = trimmed.to_lowercase().contains("insight");
            continue;
        }
        if in_section {
            if let Some(bullet) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                let bullet = bullet.trim();
                if !bullet.is_empty() && insights.len() < 10 {
                    insights.push(bullet.to_string());
                }
            } else if !trimmed.is_empty() && insights.is_empty() {
                // Skip intro prose before the first bullet.
            } else if trimmed.is_empty() && !insights.is_empty() {
                break;
            }
        }
    }
    insights
}

/// Deep Dive: plan sub-questions, run each as a Light pass (at most two at
/// once), then merge bottom-up with global citation numbers.
#[allow(clippy::too_many_arguments)]
pub async fn run_deep<P, T, S>(
    db: el_db::Database,
    tools: Arc<T>,
    sink: Arc<S>,
    cancel: CancellationToken,
    provider: Arc<P>,
    model: String,
    session_id: String,
    query: String,
    config: RunConfig,
    calls: Arc<CallBudget>,
) -> AppResult<String>
where
    P: LlmProvider + 'static,
    T: ResearchTools + 'static,
    S: EventSink + 'static,
{
    check_cancelled(&cancel, &session_id)?;
    let plan_step = record_step(
        &db,
        &*sink,
        &session_id,
        None,
        "Plan sub-questions",
        "in_progress",
    )?;
    let memory = {
        let conn = db.connect()?;
        build_context(
            &conn,
            config.project_id.as_deref(),
            config.session_memory.as_deref(),
        )?
    };
    let plan_raw = cancellable(
        &cancel,
        &session_id,
        llm_call(
            &db,
            &*provider,
            &model,
            &session_id,
            vec![ChatMessage::user(&planner_prompt(&query, &memory))],
            Some(500),
            &calls,
        ),
    )
    .await
    .inspect_err(|_| {
        finish_step(&db, &*sink, &session_id, &plan_step.id, "failed");
    })?;
    let mut sub_questions = parse_sub_questions(&plan_raw);
    if sub_questions.is_empty() {
        // The planner did not produce usable JSON: degrade to one Light pass
        // rather than failing the session.
        finish_step(&db, &*sink, &session_id, &plan_step.id, "completed");
        return run_light(
            db, tools, sink, cancel, provider, model, session_id, None, query, config, calls,
        )
        .await
        .map(|outcome| outcome.summary);
    }
    sub_questions.truncate(MAX_SUB_QUESTIONS);
    finish_step(&db, &*sink, &session_id, &plan_step.id, "completed");

    let semaphore = Arc::new(Semaphore::new(DEEP_PARALLELISM));
    let mut handles = Vec::new();
    for sub in sub_questions {
        check_cancelled(&cancel, &session_id)?;
        let step = record_step(
            &db,
            &*sink,
            &session_id,
            None,
            &format!("Sub-question: {sub}"),
            "in_progress",
        )?;
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Internal("deep-run semaphore closed".to_string()))?;
        let (
            task_db,
            task_tools,
            task_sink,
            task_cancel,
            task_provider,
            task_session,
            task_step,
            task_sub,
            task_model,
            task_config,
            task_calls,
        ) = (
            db.clone(),
            Arc::clone(&tools),
            Arc::clone(&sink),
            cancel.clone(),
            Arc::clone(&provider),
            session_id.clone(),
            step.id.clone(),
            sub.clone(),
            model.clone(),
            config.clone(),
            Arc::clone(&calls),
        );
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let outcome = run_light(
                task_db,
                task_tools,
                task_sink,
                task_cancel,
                task_provider,
                task_model,
                task_session,
                Some(task_step.clone()),
                task_sub.clone(),
                task_config,
                task_calls,
            )
            .await;
            (task_sub, task_step, outcome)
        }));
    }

    // Merge with global citation numbers: dedupe sources across children,
    // rewrite each summary onto the global list, then synthesize once.
    let mut global_sources: Vec<SourceRef> = Vec::new();
    let mut sections: Vec<String> = Vec::new();
    let mut failed = 0usize;
    for handle in handles {
        // Abandon the join (not the child) on cancel: children share the
        // token and wind down at their next checkpoint on their own.
        let joined = cancellable(&cancel, &session_id, async {
            handle
                .await
                .map_err(|err| AppError::Internal(format!("deep child panicked: {err}")))
        })
        .await?;
        let (sub, step_id, outcome) = joined;
        match outcome {
            Ok(child) => {
                let mut mapping = Vec::with_capacity(child.sources.len());
                for source in &child.sources {
                    let position = global_sources
                        .iter()
                        .position(|existing| existing.url == source.url);
                    let global = match position {
                        Some(index) => index + 1,
                        None => {
                            global_sources.push(source.clone());
                            global_sources.len()
                        }
                    };
                    mapping.push(global);
                }
                sections.push(format!(
                    "### {sub}\n{}",
                    renumber_citations(&child.summary, &mapping)
                ));
                finish_step(&db, &*sink, &session_id, &step_id, "completed");
            }
            Err(err) => {
                failed += 1;
                tracing::warn!("deep child {sub:?} failed: {err}");
                finish_step(&db, &*sink, &session_id, &step_id, "failed");
                if is_cancelled(&err) {
                    check_cancelled(&cancel, &session_id)?;
                }
            }
        }
    }
    if sections.is_empty() {
        return Err(AppError::Internal(format!(
            "all {failed} sub-questions failed; partial steps kept"
        )));
    }
    check_cancelled(&cancel, &session_id)?;

    let merge_step = record_step(
        &db,
        &*sink,
        &session_id,
        None,
        "Merge findings",
        "in_progress",
    )?;
    let numbered: Vec<NumberedSource> = global_sources
        .iter()
        .enumerate()
        .map(|(index, source)| NumberedSource {
            number: index + 1,
            url: source.url.clone(),
            title: source.title.clone(),
            text: String::new(),
        })
        .collect();
    let merged_prompt = format!(
        "You are a careful research assistant merging findings from sub-questions on one query. \
         Write a unified answer to the original query using ONLY the section summaries below, \
         keeping their citation markers (they refer to the source list). End with a \
         '### Cross-source insights' section of bullet points capturing observations that \
         combine multiple sources.\n\nOriginal query: {query}\n\n{sections}\n\nSources:\n{source_list}",
        query = query,
        sections = sections.join("\n\n"),
        source_list = numbered
            .iter()
            .map(|source| format!("[{}] {} — {}", source.number, source.title, source.url))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    // Bound the merge prompt by source count, not text (texts live in children).
    let merged = cancellable(
        &cancel,
        &session_id,
        llm_call(
            &db,
            &*provider,
            &model,
            &session_id,
            vec![ChatMessage::user(&merged_prompt)],
            Some(2000),
            &calls,
        ),
    )
    .await
    .inspect_err(|_| {
        finish_step(&db, &*sink, &session_id, &merge_step.id, "failed");
    })?;
    let (summary, has_citations) = validate_citations(&merged, global_sources.len().max(1));
    let summary = if has_citations {
        summary
    } else {
        format!("{summary}\n\n(No verifiable citations; treat claims as unverified.)")
    };
    store_result(&db, &*sink, &session_id, "summary", &summary, None, None)?;
    for insight in extract_insights(&summary) {
        store_result(&db, &*sink, &session_id, "insight", &insight, None, None)?;
    }
    finish_step(&db, &*sink, &session_id, &merge_step.id, "completed");
    Ok(summary)
}
