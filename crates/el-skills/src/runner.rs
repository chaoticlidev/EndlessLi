//! Skill runner (task 10.2): render a skill's template and run it through the
//! configured LLM.
//!
//! The runner owns no provider construction; the shell supplies a configured
//! [`LlmProvider`] and model, so keys never leave the shell. Rendering is pure
//! and testable without a network.

use el_core::AppResult;
use el_llm::{ChatMessage, LlmProvider};

use crate::skill::{render_template, SkillDoc};
use crate::ParamValue;

/// Faithfulness over flair: skills should follow their instructions closely.
const SKILL_TEMPERATURE: f32 = 0.2;

/// Room for a substantial generated document without unbounded cost. Reasoning
/// models count their hidden tokens against this budget, so leave headroom.
const SKILL_MAX_TOKENS: u32 = 4096;

/// The result of running one skill.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SkillRunOutcome {
    pub skill: String,
    pub model: String,
    /// The fully rendered prompt that was sent (for transparency).
    pub prompt: String,
    pub output: String,
    pub usage: el_llm::ChatUsage,
    pub budget_warning: Option<String>,
}

/// Renders `doc` with `values` and completes it. Token usage is logged and the
/// budget is enforced by [`el_llm::run_prompt`].
pub async fn run_skill<P: LlmProvider>(
    db: &el_db::Database,
    provider: &P,
    model: &str,
    doc: &SkillDoc,
    values: &[ParamValue],
) -> AppResult<SkillRunOutcome> {
    let prompt = render_template(&doc.template, &doc.parameters, values)?;
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt(doc),
        },
        ChatMessage::user(&prompt),
    ];
    let outcome = el_llm::run_prompt(
        db,
        provider,
        model,
        messages,
        Some(SKILL_MAX_TOKENS),
        Some(SKILL_TEMPERATURE),
        None,
    )
    .await?;
    let output = outcome.text.trim().to_string();
    if output.is_empty() {
        // A reasoning model can spend the whole output budget thinking and
        // emit nothing. Surface that as a failure instead of showing an empty
        // result the user cannot explain.
        return Err(el_core::AppError::Internal(
            "the model returned no text (it may have spent its whole output budget on internal \
             reasoning); try again, shorten the input, or switch models"
                .to_string(),
        ));
    }
    Ok(SkillRunOutcome {
        skill: doc.name.clone(),
        model: model.to_string(),
        prompt,
        output,
        usage: outcome.usage,
        budget_warning: outcome.budget_warning,
    })
}

/// Frames the skill as a system instruction so a multi-turn provider keeps it
/// in context.
fn system_prompt(doc: &SkillDoc) -> String {
    if doc.description.is_empty() {
        format!(
            "You are executing the '{}' skill. Follow the user's instructions exactly.",
            doc.name
        )
    } else {
        format!(
            "You are executing the '{}' skill: {}. Follow the user's instructions exactly.",
            doc.name, doc.description
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records the messages it was asked to complete, so the test can assert
    /// the rendered prompt actually reached the provider.
    struct RecordingProvider {
        seen: Mutex<Vec<ChatMessage>>,
    }

    impl LlmProvider for RecordingProvider {
        fn name(&self) -> &'static str {
            "recording"
        }

        async fn complete_stream(
            &self,
            request: el_llm::ChatRequest,
            chunks: tokio::sync::mpsc::UnboundedSender<String>,
        ) -> AppResult<el_llm::ChatUsage> {
            self.seen.lock().expect("lock").extend(request.messages);
            let _ = chunks.send("DONE".to_string());
            Ok(el_llm::ChatUsage {
                tokens_in: 7,
                tokens_out: 2,
                estimated: false,
            })
        }
    }

    #[tokio::test]
    async fn renders_and_completes_a_skill() {
        let (db, path) = el_db::Database::open_ephemeral().expect("open");
        let provider = RecordingProvider {
            seen: Mutex::new(Vec::new()),
        };
        let doc = crate::skill::parse_skill(
            "---\nname: Echo\ndescription: Echo input\nparameters:\n  - name: text\n    required: true\n---\nSay {{text}}",
        )
        .expect("parse");
        let outcome = run_skill(
            &db,
            &provider,
            "stub-model",
            &doc,
            &[ParamValue {
                name: "text".to_string(),
                value: "hello".to_string(),
            }],
        )
        .await
        .expect("run");
        assert_eq!(outcome.output, "DONE");
        assert_eq!(outcome.prompt, "Say hello");
        assert_eq!(outcome.usage.tokens_in, 7);

        let seen = provider.seen.lock().expect("lock");
        assert_eq!(seen.len(), 2);
        assert!(seen[0].content.contains("Echo"));
        assert_eq!(seen[1].content, "Say hello");
        drop(seen);

        let conn = db.connect().expect("connect");
        assert_eq!(
            el_db::repo::cache::sum_since(&conn, 0).expect("sum"),
            (7, 2)
        );
        drop(conn);
        drop(db);
        std::fs::remove_file(path).ok();
    }

    /// A reasoning model that burns its whole budget and emits nothing.
    struct EmptyProvider;

    impl LlmProvider for EmptyProvider {
        fn name(&self) -> &'static str {
            "empty"
        }

        async fn complete_stream(
            &self,
            _request: el_llm::ChatRequest,
            _chunks: tokio::sync::mpsc::UnboundedSender<String>,
        ) -> AppResult<el_llm::ChatUsage> {
            Ok(el_llm::ChatUsage {
                tokens_in: 5,
                tokens_out: 4096,
                estimated: false,
            })
        }
    }

    #[tokio::test]
    async fn empty_output_is_an_error() {
        let (db, path) = el_db::Database::open_ephemeral().expect("open");
        let doc = crate::skill::parse_skill("---\nname: X\n---\nDo it").expect("parse");
        let err = run_skill(&db, &EmptyProvider, "stub-model", &doc, &[])
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("returned no text"));
        drop(db);
        std::fs::remove_file(path).ok();
    }
}
