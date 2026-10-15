//! Token counting: exact for known models, labeled estimates otherwise.
//!
//! Counts run before every send so budgets gate on real numbers when the
//! tokenizer is known, and on a marked estimate otherwise. Anthropic and
//! DeepSeek models fall back to the `cl100k` tokenizer; their counts are
//! close in practice but must never be presented as exact.

/// A token count plus whether it is exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenEstimate {
    pub tokens: usize,
    /// False only when the model's own tokenizer was used.
    pub estimated: bool,
}

/// Counts one text with the model's tokenizer, or `cl100k` as fallback.
pub fn estimate_text(model: &str, text: &str) -> TokenEstimate {
    match tiktoken_rs::bpe_for_model(model) {
        Ok(encoding) => TokenEstimate {
            tokens: encoding.encode_with_special_tokens(text).len(),
            estimated: false,
        },
        Err(_) => {
            let encoding = tiktoken_rs::cl100k_base().expect("cl100k tokenizer is bundled");
            TokenEstimate {
                tokens: encoding.encode_with_special_tokens(text).len(),
                estimated: true,
            }
        }
    }
}

/// Counts a message list with per-message framing overhead (4 tokens each
/// plus 2 for the reply primer, following the OpenAI counting recipe).
pub fn estimate_messages(model: &str, messages: &[(String, String)]) -> TokenEstimate {
    let mut tokens = 2usize;
    let mut estimated = false;
    for (role, content) in messages {
        let role_estimate = estimate_text(model, role);
        let content_estimate = estimate_text(model, content);
        estimated |= role_estimate.estimated || content_estimate.estimated;
        tokens += 4 + role_estimate.tokens + content_estimate.tokens;
    }
    TokenEstimate { tokens, estimated }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_model_is_exact() {
        let estimate = estimate_text("gpt-4o", "Hello world");
        assert!(!estimate.estimated);
        assert!(estimate.tokens > 0);
    }

    #[test]
    fn unknown_model_falls_back_labeled() {
        let estimate = estimate_text("claude-opus-99", "Hello world");
        assert!(estimate.estimated);
        assert!(estimate.tokens > 0);
    }

    #[test]
    fn empty_text_counts_nothing() {
        let estimate = estimate_text("gpt-4o", "");
        assert_eq!(estimate.tokens, 0);
    }
}
