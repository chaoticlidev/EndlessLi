//! Language-model providers over HTTPS (rustls).
//!
//! One trait, three implementations: any OpenAI-compatible endpoint
//! (configurable base URL), Anthropic Messages, and DeepSeek (which speaks
//! the OpenAI protocol against its own endpoint). Streaming is the only wire
//! mode; unary completions collect the stream.

use el_core::{AppError, AppResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

/// One chat message.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    /// A user-role message.
    pub fn user(content: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: content.to_string(),
        }
    }
}

/// What to send: model, messages, and optional decoding knobs.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

/// Token counts for one call. `estimated` is true when the server reported
/// no usage and the client counted instead.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ChatUsage {
    #[specta(type = specta_typescript::Number)]
    pub tokens_in: i64,
    #[specta(type = specta_typescript::Number)]
    pub tokens_out: i64,
    pub estimated: bool,
}

/// A finished completion.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub text: String,
    pub usage: ChatUsage,
}

/// A provider speaks one HTTP dialect. Implementations must be safe to share
/// across tasks; the HTTP client is cheap to clone.
///
/// Native `async fn` is used deliberately instead of desugared futures: every
/// implementor lives in this crate, and the futures only capture `Send` data
/// (`&self` with `Send + Sync` receivers, owned requests, channels).
#[allow(async_fn_in_trait)]
pub trait LlmProvider: Send + Sync {
    /// Stable id used in logs and the `token_usage` table.
    fn name(&self) -> &'static str;

    /// Streams chunks into `chunks` and resolves with the call's usage.
    /// Callers forward chunks to the UI or drop them; the channel is
    /// unbounded so producers never block.
    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage>;

    /// Collects the stream into one response.
    async fn complete(&self, request: ChatRequest) -> AppResult<ChatResponse> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let usage = self.complete_stream(request, tx).await?;
        let mut text = String::new();
        while let Some(chunk) = rx.recv().await {
            text.push_str(&chunk);
        }
        Ok(ChatResponse { text, usage })
    }
}

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|err| AppError::Internal(format!("build HTTP client: {err}")))
}

fn status_error(provider: &str, status: reqwest::StatusCode, body: &str) -> AppError {
    let snippet: String = body.chars().take(300).collect();
    AppError::Internal(format!("llm ({provider}) HTTP {status}: {snippet}"))
}

/// One parsed SSE payload shared by both wire dialects.
#[derive(Debug)]
struct ParsedChunk {
    /// Text to emit downstream, if any.
    piece: Option<String>,
    /// True when the turn finished.
    done: bool,
    /// Server-reported usage, when the payload carries it.
    usage: Option<(i64, i64)>,
}
/// Splits a byte stream into lines, handling CR and LF endings.
fn sse_lines(buffer: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    buffer.extend_from_slice(chunk);
    let mut lines = Vec::new();
    while let Some(pos) = buffer.iter().position(|byte| *byte == b'\n') {
        let raw: Vec<u8> = buffer.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&raw);
        lines.push(line.trim_end_matches(['\r', '\n']).to_string());
    }
    lines
}

/// An OpenAI-compatible endpoint: OpenAI itself, or any server speaking the
/// chat-completions dialect (custom base URLs welcome).
pub struct OpenAiCompatible {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiCompatible {
    /// Builds a client. A blank key fails fast with a clear message.
    pub fn new(base_url: &str, api_key: &str) -> AppResult<Self> {
        if api_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "API key must not be blank".to_string(),
            ));
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            client: http_client()?,
        })
    }
}

impl LlmProvider for OpenAiCompatible {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage> {
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
        });
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|err| AppError::Internal(format!("llm (openai) request: {err}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(status_error("openai", status, &text));
        }
        let mut buffer = Vec::new();
        let mut stream = response.bytes_stream();
        let mut text = String::new();
        let mut reported: Option<(i64, i64)> = None;
        while let Some(item) = stream.next().await {
            let bytes = item.map_err(|err| AppError::Internal(format!("llm stream: {err}")))?;
            for line in sse_lines(&mut buffer, &bytes) {
                let payload = line.strip_prefix("data:").map(str::trim);
                match payload {
                    None | Some("") => {}
                    Some("[DONE]") => break,
                    Some(json) => {
                        let value: serde_json::Value =
                            serde_json::from_str(json).map_err(|err| {
                                AppError::Internal(format!("llm (openai) bad chunk: {err}"))
                            })?;
                        let chunk = parse_openai_chunk(&value);
                        if let Some(piece) = chunk.piece {
                            text.push_str(&piece);
                            let _ = chunks.send(piece);
                        }
                        if let Some(usage) = chunk.usage {
                            reported = Some(usage);
                        }
                        if chunk.done {
                            break;
                        }
                    }
                }
            }
        }
        Ok(match reported {
            Some((tokens_in, tokens_out)) => ChatUsage {
                tokens_in,
                tokens_out,
                estimated: false,
            },
            None => fallback_usage(&request, &text),
        })
    }
}

/// Estimates usage when the server reported none: input from the request,
/// output from the received text. Always labeled estimated.
fn fallback_usage(request: &ChatRequest, text: &str) -> ChatUsage {
    let tokens_in = crate::tokens::estimate_messages(
        &request.model,
        &request
            .messages
            .iter()
            .map(|message| (message.role.clone(), message.content.clone()))
            .collect::<Vec<_>>(),
    );
    let tokens_out = crate::tokens::estimate_text(&request.model, text);
    ChatUsage {
        tokens_in: tokens_in.tokens as i64,
        tokens_out: tokens_out.tokens as i64,
        estimated: tokens_in.estimated || tokens_out.estimated,
    }
}

/// Reads one OpenAI SSE data payload: an emitted text piece, whether the
/// turn finished, and server-reported usage when present.
fn parse_openai_chunk(value: &serde_json::Value) -> ParsedChunk {
    let piece = value
        .pointer("/choices/0/delta/content")
        .and_then(|content| content.as_str())
        .map(str::to_string);
    let done = value
        .pointer("/choices/0/finish_reason")
        .and_then(|reason| reason.as_str())
        .is_some_and(|reason| reason != "null" && !reason.is_empty());
    let usage = value.get("usage").and_then(|usage| {
        let tokens_in = usage.get("prompt_tokens")?.as_i64()?;
        let tokens_out = usage.get("completion_tokens")?.as_i64()?;
        Some((tokens_in, tokens_out))
    });
    ParsedChunk { piece, done, usage }
}

/// Anthropic Messages dialect with mandatory version header.
pub struct Anthropic {
    api_key: String,
    client: reqwest::Client,
}

/// Required API version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

impl Anthropic {
    /// Builds a client. A blank key fails fast with a clear message.
    pub fn new(api_key: &str) -> AppResult<Self> {
        if api_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "API key must not be blank".to_string(),
            ));
        }
        Ok(Self {
            api_key: api_key.to_string(),
            client: http_client()?,
        })
    }
}

impl LlmProvider for Anthropic {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage> {
        // Anthropic rejects `system` inside messages; hoist it instead.
        let mut system_parts = Vec::new();
        let mut messages = Vec::new();
        for message in &request.messages {
            if message.role == "system" {
                system_parts.push(message.content.clone());
            } else {
                messages.push(message);
            }
        }
        let body = serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens.unwrap_or(1024),
            "system": system_parts.join("\n\n"),
            "messages": messages,
            "temperature": request.temperature,
            "stream": true,
        });
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|err| AppError::Internal(format!("llm (anthropic) request: {err}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(status_error("anthropic", status, &text));
        }
        let mut buffer = Vec::new();
        let mut stream = response.bytes_stream();
        let mut event_name: Option<String> = None;
        let mut text = String::new();
        let mut reported: Option<(i64, i64)> = None;
        while let Some(item) = stream.next().await {
            let bytes = item.map_err(|err| AppError::Internal(format!("llm stream: {err}")))?;
            for line in sse_lines(&mut buffer, &bytes) {
                if let Some(name) = line.strip_prefix("event:") {
                    event_name = Some(name.trim().to_string());
                } else if let Some(json) = line.strip_prefix("data:").map(str::trim) {
                    if json.is_empty() {
                        continue;
                    }
                    let value: serde_json::Value = serde_json::from_str(json).map_err(|err| {
                        AppError::Internal(format!("llm (anthropic) bad chunk: {err}"))
                    })?;
                    let chunk = parse_anthropic_event(event_name.as_deref().unwrap_or(""), &value)?;
                    if let Some(piece) = chunk.piece {
                        text.push_str(&piece);
                        let _ = chunks.send(piece);
                    }
                    if let Some(usage) = chunk.usage {
                        reported = Some(usage);
                    }
                    if chunk.done {
                        break;
                    }
                    event_name = None;
                }
            }
        }
        Ok(match reported {
            Some((tokens_in, tokens_out)) => ChatUsage {
                tokens_in,
                tokens_out,
                estimated: false,
            },
            None => fallback_usage(&request, &text),
        })
    }
}

/// Reads one Anthropic SSE event: emitted text, whether the turn finished,
/// and server-reported usage when present.
fn parse_anthropic_event(event: &str, value: &serde_json::Value) -> AppResult<ParsedChunk> {
    match event {
        "content_block_delta" => {
            let piece = value
                .pointer("/delta/text")
                .and_then(|text| text.as_str())
                .map(str::to_string);
            Ok(ParsedChunk {
                piece,
                done: false,
                usage: None,
            })
        }
        "message_delta" => {
            let usage = value.get("usage").and_then(|usage| {
                let tokens_in = usage.get("input_tokens")?.as_i64().unwrap_or(0);
                let tokens_out = usage.get("output_tokens")?.as_i64()?;
                Some((tokens_in, tokens_out))
            });
            Ok(ParsedChunk {
                piece: None,
                done: false,
                usage,
            })
        }
        "message_stop" => Ok(ParsedChunk {
            piece: None,
            done: true,
            usage: None,
        }),
        "error" => {
            let message = value
                .pointer("/error/message")
                .and_then(|message| message.as_str())
                .unwrap_or("unknown anthropic error");
            Err(AppError::Internal(format!(
                "llm (anthropic) error: {message}"
            )))
        }
        _ => Ok(ParsedChunk {
            piece: None,
            done: false,
            usage: None,
        }),
    }
}

/// DeepSeek speaks the OpenAI protocol against its own endpoint.
pub struct DeepSeek {
    inner: OpenAiCompatible,
}

/// Default DeepSeek endpoint.
pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";

impl DeepSeek {
    /// Builds a client against the DeepSeek endpoint.
    pub fn new(api_key: &str) -> AppResult<Self> {
        Ok(Self {
            inner: OpenAiCompatible::new(DEEPSEEK_BASE_URL, api_key)?,
        })
    }
}

impl LlmProvider for DeepSeek {
    fn name(&self) -> &'static str {
        "deepseek"
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage> {
        self.inner.complete_stream(request, chunks).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_delta_chunk_parses() {
        let value = serde_json::json!({
            "choices": [{ "delta": { "content": "Hello" }, "finish_reason": null }]
        });
        let chunk = parse_openai_chunk(&value);
        assert_eq!(chunk.piece.as_deref(), Some("Hello"));
        assert!(!chunk.done);
        assert!(chunk.usage.is_none());
    }

    #[test]
    fn openai_stop_chunk_parses_with_usage() {
        let value = serde_json::json!({
            "choices": [{ "delta": {}, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 4 }
        });
        let chunk = parse_openai_chunk(&value);
        assert!(chunk.piece.is_none());
        assert!(chunk.done);
        assert_eq!(chunk.usage, Some((10, 4)));
    }

    #[test]
    fn anthropic_text_delta_parses() {
        let value = serde_json::json!({ "delta": { "text": "Hi" } });
        let chunk = parse_anthropic_event("content_block_delta", &value).expect("parse");
        assert_eq!(chunk.piece.as_deref(), Some("Hi"));
        assert!(!chunk.done);
        assert!(chunk.usage.is_none());
    }

    #[test]
    fn anthropic_usage_delta_parses() {
        let value = serde_json::json!({ "usage": { "input_tokens": 8, "output_tokens": 3 } });
        let chunk = parse_anthropic_event("message_delta", &value).expect("parse");
        assert_eq!(chunk.usage, Some((8, 3)));
    }

    #[test]
    fn anthropic_error_surfaces() {
        let value = serde_json::json!({ "error": { "message": "bad key" } });
        let err = parse_anthropic_event("error", &value).expect_err("must fail");
        assert!(err.to_string().contains("bad key"));
    }

    #[test]
    fn sse_lines_split_crlf() {
        let mut buffer = Vec::new();
        let lines = sse_lines(&mut buffer, b"data: {\"a\":1}\r\ndata: [DONE]\npartial");
        assert_eq!(
            lines,
            vec!["data: {\"a\":1}".to_string(), "data: [DONE]".to_string()]
        );
        assert_eq!(buffer, b"partial");
    }

    #[test]
    fn blank_keys_rejected() {
        assert!(OpenAiCompatible::new("https://x.test", "  ").is_err());
        assert!(Anthropic::new("").is_err());
        assert!(DeepSeek::new("").is_err());
    }
}
