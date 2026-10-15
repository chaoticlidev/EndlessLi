//! `el-llm`: provider trait, token counting, and budget enforcement.
//!
//! Every LLM call (research synthesis, skills, memory suggestions, and the
//! Settings self-test) flows through [`run_test_prompt`]-style logic: count
//! first, check budgets, send, sanitize, log usage. Providers implement
//! [`provider::LlmProvider`]; tests use stub providers, never the network.

pub mod budgets;
pub mod provider;
pub mod tokens;

pub use budgets::{check_budget, BudgetCheck};
pub use el_core::{AppError, AppResult};
pub use provider::{
    Anthropic, ChatMessage, ChatRequest, ChatResponse, ChatUsage, DeepSeek, LlmProvider,
    OpenAiCompatible, ANTHROPIC_VERSION, DEEPSEEK_BASE_URL,
};
pub use tokens::{estimate_messages, estimate_text, TokenEstimate};

/// Outcome of the Settings self-test prompt, returned through IPC.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TestOutcome {
    pub text: String,
    #[specta(type = specta_typescript::Number)]
    pub tokens_in: i64,
    #[specta(type = specta_typescript::Number)]
    pub tokens_out: i64,
    pub estimated: bool,
    pub budget_warning: Option<String>,
}

/// Runs one prompt end to end: budget-check the estimate, send, sanitize the
/// output, and log usage. Shared by the `api_key_test` command and, later,
/// research and skills.
///
/// Takes the pool (not a connection) so no `!Send` handle is held across
/// awaits: budget state is read before sending, usage is logged after, each
/// on its own short-lived checkout.
pub async fn run_test_prompt<P: LlmProvider>(
    db: &el_db::Database,
    provider: &P,
    model: &str,
    session_id: Option<&str>,
) -> AppResult<TestOutcome> {
    let messages = vec![ChatMessage::user("Reply with exactly this word: OK")];
    let estimate = estimate_messages(
        model,
        &messages
            .iter()
            .map(|message| (message.role.clone(), message.content.clone()))
            .collect::<Vec<_>>(),
    );
    let warning = {
        let conn = db.connect()?;
        check_budget(&conn, estimate.tokens as i64)?.warning
    };
    let request = ChatRequest {
        model: model.to_string(),
        messages,
        max_tokens: Some(16),
        temperature: Some(0.0),
    };
    let response = provider.complete(request).await?;
    let text = el_security::sanitize_llm_output(&response.text);
    {
        let conn = db.connect()?;
        el_db::repo::cache::log_usage(
            &conn,
            session_id,
            provider.name(),
            model,
            response.usage.tokens_in,
            response.usage.tokens_out,
        )?;
    }
    Ok(TestOutcome {
        text,
        tokens_in: response.usage.tokens_in,
        tokens_out: response.usage.tokens_out,
        estimated: response.usage.estimated,
        budget_warning: warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic provider: no network, fixed output and usage.
    struct StubProvider;

    impl LlmProvider for StubProvider {
        fn name(&self) -> &'static str {
            "stub"
        }

        async fn complete_stream(
            &self,
            _request: ChatRequest,
            chunks: tokio::sync::mpsc::UnboundedSender<String>,
        ) -> AppResult<ChatUsage> {
            let _ = chunks.send("OK".to_string());
            Ok(ChatUsage {
                tokens_in: 12,
                tokens_out: 2,
                estimated: false,
            })
        }
    }

    #[tokio::test]
    async fn test_prompt_logs_usage_end_to_end() {
        let (db, path) = el_db::Database::open_ephemeral().expect("open");
        let outcome = run_test_prompt(&db, &StubProvider, "stub-model", None)
            .await
            .expect("run");
        assert_eq!(outcome.text, "OK");
        assert_eq!((outcome.tokens_in, outcome.tokens_out), (12, 2));
        let conn = db.connect().expect("connect");
        let (logged_in, logged_out) = el_db::repo::cache::sum_since(&conn, 0).expect("sum");
        assert_eq!((logged_in, logged_out), (12, 2));
        drop(conn);
        drop(db);
        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn test_prompt_respects_budget_stop() {
        let (db, path) = el_db::Database::open_ephemeral().expect("open");
        let conn = db.connect().expect("connect");
        el_db::repo::cache::set_setting(&conn, budgets::SETTING_DAILY_TOKENS, "1").expect("set");
        drop(conn);
        let err = run_test_prompt(&db, &StubProvider, "stub-model", None)
            .await
            .expect_err("must block");
        assert!(err.to_string().contains("budget exceeded"));
        // Blocked calls log nothing.
        let conn = db.connect().expect("connect");
        assert_eq!(
            el_db::repo::cache::sum_since(&conn, 0).expect("sum"),
            (0, 0)
        );
        drop(conn);
        drop(db);
        std::fs::remove_file(path).ok();
    }
}
