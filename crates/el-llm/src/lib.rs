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
    Anthropic, AnyProvider, ChatMessage, ChatRequest, ChatResponse, ChatUsage, DeepSeek,
    LlmProvider, OpenAiCompatible, ANTHROPIC_VERSION, DEEPSEEK_BASE_URL, OPENAI_BASE_URL,
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

/// A completion plus its usage and any budget warning, from [`run_prompt`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PromptOutcome {
    pub text: String,
    pub usage: ChatUsage,
    pub budget_warning: Option<String>,
}

/// Sends one chat request end to end: budget-check the estimate, send, log the
/// usage, and sanitize the output. The single funnel every LLM call goes
/// through (the Settings self-test, research, and skills).
///
/// Takes the pool (not a connection) so no `!Send` handle is held across
/// awaits: budget state is read before sending, usage is logged after, each on
/// its own short-lived checkout. A budget stop propagates as an error and logs
/// nothing.
pub async fn run_prompt<P: LlmProvider>(
    db: &el_db::Database,
    provider: &P,
    model: &str,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    session_id: Option<&str>,
) -> AppResult<PromptOutcome> {
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
        max_tokens,
        temperature,
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
    Ok(PromptOutcome {
        text,
        usage: response.usage,
        budget_warning: warning,
    })
}

/// Runs the Settings self-test prompt (a fixed one-word reply) through
/// [`run_prompt`] and reshapes the outcome for the Settings UI.
pub async fn run_test_prompt<P: LlmProvider>(
    db: &el_db::Database,
    provider: &P,
    model: &str,
    session_id: Option<&str>,
) -> AppResult<TestOutcome> {
    let messages = vec![ChatMessage::user("Reply with exactly this word: OK")];
    let outcome = run_prompt(
        db,
        provider,
        model,
        messages,
        Some(16),
        Some(0.0),
        session_id,
    )
    .await?;
    Ok(TestOutcome {
        text: outcome.text,
        tokens_in: outcome.usage.tokens_in,
        tokens_out: outcome.usage.tokens_out,
        estimated: outcome.usage.estimated,
        budget_warning: outcome.budget_warning,
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
