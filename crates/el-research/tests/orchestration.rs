//! Orchestration with stub tools and a stub provider: no network, no sidecar.
//! Covers Light persistence, Deep merge with global citations, and cancel.

use std::sync::Arc;

use el_core::{AppError, AppResult, NewSession};
use el_llm::{ChatRequest, ChatUsage, LlmProvider};
use el_research::research::{run_deep, run_light, CallBudget, NullSink, RunConfig};
use el_research::tools::{ClipDoc, ResearchTools, SearchHit};
use tokio_util::sync::CancellationToken;

struct TestDb {
    db: el_db::Database,
    path: std::path::PathBuf,
}

impl TestDb {
    fn setup() -> Self {
        let (db, path) = el_db::Database::open_ephemeral().expect("open test database");
        Self { db, path }
    }

    fn session(&self, mode: &str) -> String {
        let conn = self.db.connect().expect("connect");
        let session = el_db::repo::research::create_session(
            &conn,
            &NewSession {
                query: "test query".to_string(),
                mode: mode.to_string(),
                project_id: None,
                folder_id: None,
                context_memory: None,
            },
        )
        .expect("create session");
        el_db::repo::research::set_session_status(&conn, &session.id, "in_progress")
            .expect("start");
        session.id
    }

    fn finish(self) {
        drop(self.db);
        std::fs::remove_file(self.path).ok();
    }
}

struct StubTools;

impl ResearchTools for StubTools {
    #[allow(unused_variables)]
    async fn search(
        &self,
        query: &str,
        _max_results: u32,
        _providers: &[String],
        _searxng_url: Option<&str>,
        _brave_key: Option<&str>,
        _tavily_key: Option<&str>,
        _exa_key: Option<&str>,
        _keenable_key: Option<&str>,
        _firecrawl_key: Option<&str>,
        _bing_key: Option<&str>,
        _parallel_key: Option<&str>,
        _anysearch_key: Option<&str>,
        _perplexity_key: Option<&str>,
        _serpbase_key: Option<&str>,
        _you_key: Option<&str>,
        _safe_search: &str,
    ) -> AppResult<(Vec<SearchHit>, String)> {
        Ok((
            vec![
                SearchHit {
                    title: format!("First on {query}"),
                    url: "https://one.test".to_string(),
                    snippet: "first".to_string(),
                },
                SearchHit {
                    title: format!("Second on {query}"),
                    url: "https://two.test".to_string(),
                    snippet: "second".to_string(),
                },
            ],
            "stub".to_string(),
        ))
    }

    async fn clip(&self, url: &str, _max_chars: u32) -> AppResult<ClipDoc> {
        Ok(ClipDoc {
            url: url.to_string(),
            title: format!("Clipped {url}"),
            text: "Supporting content for synthesis.".to_string(),
            truncated: false,
        })
    }
}

/// Provider that plans, summarizes, and merges from canned replies.
struct ScriptedProvider;

impl LlmProvider for ScriptedProvider {
    fn name(&self) -> &'static str {
        "stub"
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage> {
        let prompt = request
            .messages
            .first()
            .map(|message| message.content.clone())
            .unwrap_or_default();
        let text = if prompt.contains("Decompose the query") {
            r#"{"sub_questions": ["first angle", "second angle"]}"#.to_string()
        } else if prompt.contains("merging findings") {
            "Merged answer [1] with [2].\n\n### Cross-source insights\n- Combined observation."
                .to_string()
        } else {
            "A summary with [1] citation.".to_string()
        };
        let _ = chunks.send(text.clone());
        Ok(ChatUsage {
            tokens_in: 10,
            tokens_out: 5,
            estimated: false,
        })
    }
}

fn config() -> RunConfig {
    RunConfig {
        providers: vec!["stub".to_string()],
        searxng_url: None,
        brave_key: None,
        tavily_key: None,
        exa_key: None,
        keenable_key: None,
        firecrawl_key: None,
        bing_key: None,
        parallel_key: None,
        anysearch_key: None,
        perplexity_key: None,
        serpbase_key: None,
        you_key: None,
        safe_search: "off".to_string(),
        clip_count: 2,
        max_context_tokens: 60000,
        llm_call_cap: 25,
        project_id: None,
        folder_id: None,
        session_memory: None,
    }
}

#[tokio::test]
async fn light_run_persists_steps_sources_and_summary() {
    let test = TestDb::setup();
    let session_id = test.session("light");
    let outcome = run_light(
        test.db.clone(),
        Arc::new(StubTools),
        Arc::new(NullSink),
        CancellationToken::new(),
        Arc::new(ScriptedProvider),
        "stub-model".to_string(),
        session_id.clone(),
        None,
        "test query".to_string(),
        config(),
        Arc::new(CallBudget::new(25)),
    )
    .await
    .expect("light run");
    assert!(outcome.summary.contains("[1]"));
    assert_eq!(outcome.sources.len(), 2);

    let conn = test.db.connect().expect("connect");
    let steps = el_db::repo::research::list_steps(&conn, &session_id).expect("steps");
    assert!(steps.iter().all(|step| step.status == "completed"));
    assert!(steps.len() >= 3);
    let results = el_db::repo::research::list_results(&conn, &session_id).expect("results");
    assert!(results.iter().any(|result| result.result_type == "summary"));
    assert_eq!(
        results
            .iter()
            .filter(|result| result.result_type == "source")
            .count(),
        2
    );
    let session = el_db::repo::research::require_session(&conn, &session_id).expect("session");
    assert_eq!((session.tokens_in, session.tokens_out), (10, 5));
    drop(conn);
    test.finish();
}

#[tokio::test]
async fn deep_run_merges_with_global_citations() {
    let test = TestDb::setup();
    let session_id = test.session("deep");
    let summary = run_deep(
        test.db.clone(),
        Arc::new(StubTools),
        Arc::new(NullSink),
        CancellationToken::new(),
        Arc::new(ScriptedProvider),
        "stub-model".to_string(),
        session_id.clone(),
        "test query".to_string(),
        config(),
        Arc::new(CallBudget::new(25)),
    )
    .await
    .expect("deep run");
    // Both children cited local [1]; the merge keeps both as global [1] and [2].
    assert!(summary.contains("[1]"), "summary: {summary}");
    assert!(summary.contains("[2]"), "summary: {summary}");

    let conn = test.db.connect().expect("connect");
    let results = el_db::repo::research::list_results(&conn, &session_id).expect("results");
    assert!(results.iter().any(|result| result.result_type == "summary"));
    assert!(results.iter().any(|result| result.result_type == "insight"));
    drop(conn);
    test.finish();
}

#[tokio::test]
async fn cancelled_run_fails_cleanly() {
    let test = TestDb::setup();
    let session_id = test.session("light");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = run_light(
        test.db.clone(),
        Arc::new(StubTools),
        Arc::new(NullSink),
        cancel,
        Arc::new(ScriptedProvider),
        "stub-model".to_string(),
        session_id.clone(),
        None,
        "test query".to_string(),
        config(),
        Arc::new(CallBudget::new(25)),
    )
    .await
    .expect_err("must cancel");
    assert!(el_research::research::is_cancelled(&err));
    assert!(matches!(err, AppError::Internal(_)));
    test.finish();
}
