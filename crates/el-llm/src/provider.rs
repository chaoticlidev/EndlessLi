//! Language-model providers over HTTPS (rustls).
//!
//! Two wire dialects cover every built-in provider: OpenAI-compatible
//! `chat/completions` (18 vendors plus user-defined endpoints) and Anthropic
//! Messages. GitHub Copilot speaks the OpenAI dialect after exchanging a GitHub
//! token for a short-lived Copilot token.
//!
//! [`AnyProvider`] is an ordered fallback chain: it tries each configured
//! target until one produces a completion, then reports which provider and
//! model served the call so usage rows stay accurate. Streaming is the wire
//! mode; unary completions collect the stream.

use el_core::{AppError, AppResult};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::future::Future;

use crate::catalog::{self, MaxTokensField, TemperaturePolicy, WireFormat};

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
///
/// `provider` and `model` identify the target that actually served the call.
/// With a fallback chain the configured first choice is not necessarily the
/// one that answered, so usage logging must use these fields.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub text: String,
    pub usage: ChatUsage,
    pub provider: String,
    pub model: String,
}

/// One chain entry: which provider, which model, and where its key lives.
///
/// The API key travels in memory only. `base_url` overrides the catalog default
/// when present; a custom endpoint always supplies one.
#[derive(Debug, Clone)]
pub struct ProviderTarget {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: String,
}

/// A provider speaks one HTTP dialect. Implementations must be safe to share
/// across tasks; the HTTP client is cheap to clone.
///
/// Methods are desugared futures with explicit `Send` bounds (rather than
/// `async fn`) so orchestration can spawn provider calls onto tasks.
pub trait LlmProvider: Send + Sync {
    /// Stable id used in logs and the `token_usage` table. Not `&'static`
    /// because user-defined endpoints have runtime ids.
    fn name(&self) -> &str;

    /// Streams chunks into `chunks` and resolves with the call's usage.
    /// Callers forward chunks to the UI or drop them; the channel is
    /// unbounded so producers never block.
    fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> impl Future<Output = AppResult<ChatUsage>> + Send + '_;

    /// Collects the stream into one response.
    fn complete(
        &self,
        request: ChatRequest,
    ) -> impl Future<Output = AppResult<ChatResponse>> + Send + '_ {
        async move {
            let provider = self.name().to_string();
            let model = request.model.clone();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let usage = self.complete_stream(request, tx).await?;
            let mut text = String::new();
            while let Some(chunk) = rx.recv().await {
                text.push_str(&chunk);
            }
            Ok(ChatResponse {
                text,
                usage,
                provider,
                model,
            })
        }
    }
}

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        // A total-request timeout truncates long generations mid-stream.
        // Bound connection setup and per-read inactivity instead.
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|err| AppError::Internal(format!("build HTTP client: {err}")))
}

fn status_error(provider: &str, status: reqwest::StatusCode, body: &str) -> AppError {
    let snippet: String = body.chars().take(300).collect();
    AppError::Internal(format!("llm ({provider}) HTTP {status}: {snippet}"))
}

/// Rejects anything that is not an HTTP(S) URL before it reaches the network
/// layer. Blocks `file://`, `data:`, and other schemes a settings value might
/// otherwise smuggle in.
fn validate_base_url(url: &str) -> AppResult<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(trimmed.to_string())
    } else {
        Err(AppError::InvalidInput(format!(
            "provider base URL must start with http:// or https://, got {url:?}"
        )))
    }
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

/// Builds the JSON body shared by every OpenAI-compatible endpoint.
///
/// Optional knobs are omitted rather than sent as `null`: several vendors
/// reject explicit nulls, and Kimi rejects any temperature but its own default.
fn openai_body(
    request: &ChatRequest,
    provider: &str,
    temperature: TemperaturePolicy,
    max_tokens_field: MaxTokensField,
) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), serde_json::json!(request.model));
    body.insert("messages".to_string(), serde_json::json!(request.messages));
    body.insert("stream".to_string(), serde_json::json!(true));
    body.insert(
        "stream_options".to_string(),
        serde_json::json!({ "include_usage": true }),
    );
    if let Some(max_tokens) = request.max_tokens {
        let field = match max_tokens_field {
            MaxTokensField::MaxTokens => "max_tokens",
            MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
        };
        body.insert(field.to_string(), serde_json::json!(max_tokens));
    }
    if temperature == TemperaturePolicy::Passthrough {
        if let Some(value) = request.temperature {
            body.insert("temperature".to_string(), serde_json::json!(value));
        }
    }
    let _ = provider;
    serde_json::Value::Object(body)
}

/// An OpenAI-compatible endpoint: OpenAI itself, or any server speaking the
/// chat-completions dialect (including user-defined endpoints).
pub struct OpenAiCompatible {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
    provider: String,
    extra_headers: HeaderMap,
    allow_blank_key: bool,
    temperature: TemperaturePolicy,
    max_tokens_field: MaxTokensField,
}

impl OpenAiCompatible {
    /// Builds a client for OpenAI itself. A blank key fails fast.
    pub fn new(base_url: &str, api_key: &str) -> AppResult<Self> {
        Self::with_options(
            base_url,
            api_key,
            "openai",
            HeaderMap::new(),
            false,
            TemperaturePolicy::Passthrough,
        )
    }

    /// Builds a client for any OpenAI-compatible endpoint.
    ///
    /// `allow_blank_key` is for local servers (Ollama, llama.cpp, LM Studio)
    /// that authenticate by network position rather than by token.
    pub fn with_options(
        base_url: &str,
        api_key: &str,
        provider: &str,
        extra_headers: HeaderMap,
        allow_blank_key: bool,
        temperature: TemperaturePolicy,
    ) -> AppResult<Self> {
        let client = http_client()?;
        Self::build(
            base_url,
            api_key,
            provider,
            extra_headers,
            allow_blank_key,
            temperature,
            client,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        base_url: &str,
        api_key: &str,
        provider: &str,
        extra_headers: HeaderMap,
        allow_blank_key: bool,
        temperature: TemperaturePolicy,
        client: reqwest::Client,
    ) -> AppResult<Self> {
        if api_key.trim().is_empty() && !allow_blank_key {
            return Err(AppError::InvalidInput(
                "API key must not be blank".to_string(),
            ));
        }
        Ok(Self {
            base_url: validate_base_url(base_url)?,
            api_key: api_key.to_string(),
            client,
            provider: provider.to_string(),
            extra_headers,
            allow_blank_key,
            temperature,
            max_tokens_field: catalog::max_tokens_field_for(provider),
        })
    }

    /// Whether this client tolerates a blank credential.
    pub fn allows_blank_key(&self) -> bool {
        self.allow_blank_key
    }
}

impl LlmProvider for OpenAiCompatible {
    fn name(&self) -> &str {
        &self.provider
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage> {
        let body = openai_body(
            &request,
            &self.provider,
            self.temperature,
            self.max_tokens_field,
        );
        let mut builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .headers(self.extra_headers.clone())
            .json(&body);
        if !self.api_key.trim().is_empty() {
            builder = builder.bearer_auth(&self.api_key);
        }
        let response = builder
            .send()
            .await
            .map_err(|err| AppError::Internal(format!("llm ({}) request: {err}", self.provider)))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(status_error(&self.provider, status, &text));
        }
        let mut buffer = Vec::new();
        let mut stream = response.bytes_stream();
        let mut text = String::new();
        let mut reported: Option<(i64, i64)> = None;
        // `[DONE]` terminates the whole stream. OpenAI sends the usage-bearing
        // chunk after `finish_reason`, so finishing on the reason would drop it.
        'stream: while let Some(item) = stream.next().await {
            let bytes = item.map_err(|err| AppError::Internal(format!("llm stream: {err}")))?;
            for line in sse_lines(&mut buffer, &bytes) {
                let payload = line.strip_prefix("data:").map(str::trim);
                match payload {
                    None | Some("") => {}
                    Some("[DONE]") => break 'stream,
                    Some(json) => {
                        let value: serde_json::Value =
                            serde_json::from_str(json).map_err(|err| {
                                AppError::Internal(format!(
                                    "llm ({}) bad chunk: {err}",
                                    self.provider
                                ))
                            })?;
                        if let Some(message) = extract_error_message(&value) {
                            return Err(AppError::Internal(format!(
                                "llm ({}) error: {message}",
                                self.provider
                            )));
                        }
                        let chunk = parse_openai_chunk(&value);
                        if let Some(piece) = chunk.piece {
                            text.push_str(&piece);
                            let _ = chunks.send(piece);
                        }
                        if let Some(usage) = chunk.usage {
                            reported = Some(usage);
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

/// Some compatible servers report failures inside a 200 SSE body instead of
/// using an HTTP error status.
fn extract_error_message(value: &serde_json::Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(|message| message.as_str())
                .or_else(|| error.as_str())
        })
        .map(str::to_string)
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
    base_url: String,
    client: reqwest::Client,
    provider: String,
}

impl Anthropic {
    /// Builds a client against the public Anthropic endpoint.
    pub fn new(api_key: &str) -> AppResult<Self> {
        Self::with_base("anthropic", ANTHROPIC_BASE_URL, api_key)
    }

    /// Builds a client against an explicit endpoint (proxy or gateway).
    pub fn with_base(provider: &str, base_url: &str, api_key: &str) -> AppResult<Self> {
        if api_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "API key must not be blank".to_string(),
            ));
        }
        Ok(Self {
            api_key: api_key.to_string(),
            base_url: validate_base_url(base_url)?,
            client: http_client()?,
            provider: provider.to_string(),
        })
    }
}

impl LlmProvider for Anthropic {
    fn name(&self) -> &str {
        &self.provider
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
        let mut body = serde_json::Map::new();
        body.insert("model".to_string(), serde_json::json!(request.model));
        body.insert(
            "max_tokens".to_string(),
            serde_json::json!(request.max_tokens.unwrap_or(1024)),
        );
        body.insert("messages".to_string(), serde_json::json!(messages));
        body.insert("stream".to_string(), serde_json::json!(true));
        if !system_parts.is_empty() {
            body.insert(
                "system".to_string(),
                serde_json::json!(system_parts.join("\n\n")),
            );
        }
        if let Some(temperature) = request.temperature {
            body.insert("temperature".to_string(), serde_json::json!(temperature));
        }
        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&serde_json::Value::Object(body))
            .send()
            .await
            .map_err(|err| AppError::Internal(format!("llm ({}) request: {err}", self.provider)))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(status_error(&self.provider, status, &text));
        }
        let mut buffer = Vec::new();
        let mut stream = response.bytes_stream();
        let mut event_name: Option<String> = None;
        let mut text = String::new();
        let mut reported: Option<(i64, i64)> = None;
        'stream: while let Some(item) = stream.next().await {
            let bytes = item.map_err(|err| AppError::Internal(format!("llm stream: {err}")))?;
            for line in sse_lines(&mut buffer, &bytes) {
                if let Some(name) = line.strip_prefix("event:") {
                    event_name = Some(name.trim().to_string());
                } else if let Some(json) = line.strip_prefix("data:").map(str::trim) {
                    if json.is_empty() {
                        continue;
                    }
                    let value: serde_json::Value = serde_json::from_str(json).map_err(|err| {
                        AppError::Internal(format!("llm ({}) bad chunk: {err}", self.provider))
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
                        break 'stream;
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
///
/// Retained as a named type for callers that want the vendor-specific default;
/// the catalog builds it through [`OpenAiCompatible`] like every other vendor.
pub struct DeepSeek {
    inner: OpenAiCompatible,
}

/// Default DeepSeek endpoint. DeepSeek accepts both with and without `/v1`.
pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";

/// Default OpenAI endpoint for the compatible dialect.
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// Default Anthropic endpoint for the Messages dialect.
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";

/// Required Anthropic API version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

impl DeepSeek {
    /// Builds a client against the DeepSeek endpoint.
    pub fn new(api_key: &str) -> AppResult<Self> {
        Ok(Self {
            inner: OpenAiCompatible::with_options(
                DEEPSEEK_BASE_URL,
                api_key,
                "deepseek",
                HeaderMap::new(),
                false,
                TemperaturePolicy::Passthrough,
            )?,
        })
    }
}

impl LlmProvider for DeepSeek {
    fn name(&self) -> &str {
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

/// GitHub Copilot: an OpenAI-compatible endpoint that expects IDE identity
/// headers and a Copilot token.
///
/// When the stored credential is a GitHub token it is exchanged for a Copilot
/// token at request time and cached until it expires. This keeps the Settings
/// flow to "paste one token".
pub struct Copilot {
    base_url: String,
    credential: String,
    client: reqwest::Client,
    cached: tokio::sync::Mutex<Option<CachedCopilotToken>>,
}

struct CachedCopilotToken {
    token: String,
    expires_at: i64,
}

/// GitHub token prefixes that must be exchanged for a Copilot token.
const GITHUB_TOKEN_PREFIXES: &[&str] = &["ghu_", "ghp_", "gho_", "github_pat_"];

impl Copilot {
    /// Builds a Copilot client. A blank credential fails fast.
    pub fn new(base_url: &str, credential: &str) -> AppResult<Self> {
        if credential.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "GitHub or Copilot token must not be blank".to_string(),
            ));
        }
        Ok(Self {
            base_url: validate_base_url(base_url)?,
            credential: credential.to_string(),
            client: http_client()?,
            cached: tokio::sync::Mutex::new(None),
        })
    }

    /// Returns a usable Copilot token, exchanging GitHub tokens once and
    /// reusing the result until shortly before expiry.
    async fn token(&self) -> AppResult<String> {
        if !GITHUB_TOKEN_PREFIXES
            .iter()
            .any(|prefix| self.credential.starts_with(prefix))
        {
            return Ok(self.credential.clone());
        }
        let now = chrono::Utc::now().timestamp();
        if let Some(cached) = self.cached.lock().await.as_ref() {
            if cached.expires_at == 0 || cached.expires_at - 60 > now {
                return Ok(cached.token.clone());
            }
        }
        let (token, expires_at) = exchange_github_token(&self.client, &self.credential).await?;
        *self.cached.lock().await = Some(CachedCopilotToken {
            token: token.clone(),
            expires_at,
        });
        Ok(token)
    }
}

/// IDE identity headers Copilot's inference host requires.
fn copilot_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("Editor-Version", "vscode/1.99.0"),
        ("Editor-Plugin-Version", "copilot-chat/0.26.7"),
        ("Copilot-Integration-Id", "vscode-chat"),
        ("X-GitHub-Api-Version", "2026-08-01"),
        ("Openai-Intent", "conversation-panel"),
        ("User-Agent", "GitHubCopilotChat/0.26.7"),
    ] {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(name, value);
        }
    }
    headers
}

/// Exchanges a GitHub token for a Copilot token. Returns `(token, expires_at)`
/// where `0` means "no expiry reported".
async fn exchange_github_token(
    client: &reqwest::Client,
    github_token: &str,
) -> AppResult<(String, i64)> {
    let response = client
        .get("https://api.github.com/copilot_internal/v2/token")
        .bearer_auth(github_token)
        .header("Accept", "application/json")
        .header("User-Agent", "EndlessLi")
        .header("Editor-Version", "vscode/1.99.0")
        .send()
        .await
        .map_err(|err| AppError::Internal(format!("copilot token exchange: {err}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(status_error("github-copilot auth", status, &text));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| AppError::Internal(format!("copilot token response: {err}")))?;
    let token = value
        .get("token")
        .and_then(|token| token.as_str())
        .ok_or_else(|| {
            AppError::Internal(
                "copilot token response did not include a token; use a Copilot API token instead"
                    .to_string(),
            )
        })?;
    let expires_at = value
        .get("expires_at")
        .and_then(|expires| expires.as_i64())
        .unwrap_or(0);
    Ok((token.to_string(), expires_at))
}

impl LlmProvider for Copilot {
    fn name(&self) -> &str {
        "github-copilot"
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage> {
        let token = self.token().await?;
        let inner = OpenAiCompatible::build(
            &self.base_url,
            &token,
            "github-copilot",
            copilot_headers(),
            false,
            TemperaturePolicy::Passthrough,
            self.client.clone(),
        )?;
        inner.complete_stream(request, chunks).await
    }
}

enum TargetProvider {
    OpenAi(OpenAiCompatible),
    Anthropic(Anthropic),
    Copilot(Copilot),
}

struct AnyTarget {
    provider_id: String,
    model: String,
    inner: TargetProvider,
}

/// Any configured provider behind one type, plus an ordered fallback chain.
///
/// `complete` walks the chain until one target answers; a target that fails
/// (bad key, retired model, rate limit) is logged and skipped. When every
/// target fails the error lists each attempt so the UI can show the real cause.
pub struct AnyProvider {
    targets: Vec<AnyTarget>,
}

impl AnyProvider {
    /// Builds a single provider from its parts. Keeps the original call shape
    /// used by the Settings self-test and by callers that want no fallback.
    pub fn from_parts(provider: &str, base_url: Option<&str>, api_key: &str) -> AppResult<Self> {
        Self::from_targets(vec![ProviderTarget {
            provider: provider.to_string(),
            model: String::new(),
            base_url: base_url.map(str::to_string),
            api_key: api_key.to_string(),
        }])
    }

    /// Builds the ordered chain. Targets that cannot be constructed (missing
    /// key, missing endpoint) are skipped; an error is returned only when no
    /// target is usable.
    pub fn from_targets(targets: Vec<ProviderTarget>) -> AppResult<Self> {
        if targets.is_empty() {
            return Err(AppError::InvalidInput(
                "no LLM provider is configured; add a provider in Settings".to_string(),
            ));
        }
        let mut built = Vec::new();
        let mut errors = Vec::new();
        for target in targets {
            match build_target(&target) {
                Ok(inner) => built.push(inner),
                Err(err) => {
                    tracing::warn!(
                        provider = %target.provider,
                        %err,
                        "skipping unusable LLM provider"
                    );
                    errors.push(format!("{}: {err}", target.provider));
                }
            }
        }
        if built.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "no usable LLM provider: {}",
                errors.join("; ")
            )));
        }
        Ok(Self { targets: built })
    }

    /// Number of usable targets in the chain.
    pub fn chain_len(&self) -> usize {
        self.targets.len()
    }
}

fn build_target(target: &ProviderTarget) -> AppResult<AnyTarget> {
    if target.provider.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "provider id must not be blank".to_string(),
        ));
    }
    let custom = catalog::is_custom(&target.provider);
    if custom {
        catalog::validate_custom_id(&target.provider)?;
    }
    let base_url = target
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let default = catalog::default_base_url_for(&target.provider);
            (!default.is_empty()).then(|| default.to_string())
        })
        .ok_or_else(|| {
            AppError::InvalidInput(format!(
                "no base URL configured for {}; add one in Settings",
                target.provider
            ))
        })?;
    let inner = match catalog::wire_for(&target.provider) {
        WireFormat::Anthropic => TargetProvider::Anthropic(Anthropic::with_base(
            &target.provider,
            &base_url,
            &target.api_key,
        )?),
        WireFormat::Copilot => TargetProvider::Copilot(Copilot::new(&base_url, &target.api_key)?),
        WireFormat::OpenAi => TargetProvider::OpenAi(OpenAiCompatible::with_options(
            &base_url,
            &target.api_key,
            &target.provider,
            HeaderMap::new(),
            custom,
            catalog::temperature_for(&target.provider),
        )?),
    };
    Ok(AnyTarget {
        provider_id: target.provider.clone(),
        model: target.model.clone(),
        inner,
    })
}

impl LlmProvider for AnyProvider {
    fn name(&self) -> &str {
        self.targets
            .first()
            .map(|target| target.provider_id.as_str())
            .unwrap_or("none")
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        chunks: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> AppResult<ChatUsage> {
        // Streaming cannot retry after bytes have reached the UI, so it uses
        // the first target. Every in-app call goes through `complete`, which
        // does retry the whole request.
        let target = self
            .targets
            .first()
            .ok_or_else(|| AppError::Internal("empty LLM chain".to_string()))?;
        let mut request = request;
        if !target.model.is_empty() {
            request.model = target.model.clone();
        }
        match &target.inner {
            TargetProvider::OpenAi(inner) => inner.complete_stream(request, chunks).await,
            TargetProvider::Anthropic(inner) => inner.complete_stream(request, chunks).await,
            TargetProvider::Copilot(inner) => inner.complete_stream(request, chunks).await,
        }
    }

    async fn complete(&self, request: ChatRequest) -> AppResult<ChatResponse> {
        let mut failures = Vec::new();
        for target in &self.targets {
            let mut attempt = request.clone();
            if !target.model.is_empty() {
                attempt.model = target.model.clone();
            }
            let result = match &target.inner {
                TargetProvider::OpenAi(inner) => inner.complete(attempt.clone()).await,
                TargetProvider::Anthropic(inner) => inner.complete(attempt.clone()).await,
                TargetProvider::Copilot(inner) => inner.complete(attempt.clone()).await,
            };
            match result {
                Ok(mut response) => {
                    // The concrete adapter reports its own name; the chain is
                    // authoritative about which target actually served.
                    response.provider = target.provider_id.clone();
                    response.model = attempt.model.clone();
                    if !failures.is_empty() {
                        tracing::info!(
                            provider = %target.provider_id,
                            fallbacks = failures.len(),
                            "LLM fallback succeeded"
                        );
                    }
                    return Ok(response);
                }
                Err(err) => {
                    tracing::warn!(
                        provider = %target.provider_id,
                        model = %attempt.model,
                        %err,
                        "LLM provider failed; trying the next chain entry"
                    );
                    failures.push(format!("{}/{}: {err}", target.provider_id, attempt.model));
                }
            }
        }
        Err(AppError::Internal(format!(
            "all LLM providers failed: {}",
            failures.join(" | ")
        )))
    }
}

/// Lists a provider's models from its `/models` endpoint.
///
/// Used by Settings to populate a provider's model list instead of asking the
/// user to type ids from memory. Errors are returned verbatim so a gateway that
/// lacks `/models` (Cloudflare, for one) can prompt for manual entry.
pub async fn list_models(provider: &str, base_url: &str, api_key: &str) -> AppResult<Vec<String>> {
    let base = validate_base_url(base_url)?;
    let client = http_client()?;
    let url = format!("{base}/models");
    let response = match catalog::wire_for(provider) {
        WireFormat::Anthropic => {
            if api_key.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "API key must not be blank".to_string(),
                ));
            }
            client
                .get(&url)
                .header("x-api-key", api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .send()
                .await
        }
        WireFormat::Copilot => {
            let token = exchange_github_token(&client, api_key)
                .await
                .map(|(token, _)| token)
                .or_else(|_| Ok::<String, AppError>(api_key.to_string()))?;
            client
                .get(&url)
                .bearer_auth(token)
                .headers(copilot_headers())
                .send()
                .await
        }
        WireFormat::OpenAi => {
            let mut builder = client.get(&url);
            if !api_key.trim().is_empty() {
                builder = builder.bearer_auth(api_key);
            }
            builder.send().await
        }
    }
    .map_err(|err| AppError::Internal(format!("list models for {provider}: {err}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(status_error(provider, status, &text));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| AppError::Internal(format!("list models for {provider}: {err}")))?;
    let mut models = parse_model_ids(&value);
    models.sort();
    models.dedup();
    if models.is_empty() {
        return Err(AppError::Internal(format!(
            "{provider} returned no models; add model ids manually"
        )));
    }
    Ok(models)
}

/// Accepts the shapes vendors return from `/models`: `{data:[{id}]}`,
/// `{models:[{id}]}`, `[{id}]`, or `["id"]`.
fn parse_model_ids(value: &serde_json::Value) -> Vec<String> {
    let array = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(|entries| entries.as_array())
        .or_else(|| value.as_array());
    let Some(array) = array else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|entry| {
            entry
                .get("id")
                .and_then(|id| id.as_str())
                .or_else(|| entry.get("name").and_then(|name| name.as_str()))
                .or_else(|| entry.as_str())
                .map(str::to_string)
        })
        .collect()
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
        assert!(Copilot::new("https://x.test", "").is_err());
    }

    #[test]
    fn non_http_endpoints_rejected() {
        assert!(OpenAiCompatible::new("file:///etc/passwd", "key").is_err());
        assert!(OpenAiCompatible::new("ftp://example.com", "key").is_err());
        assert!(OpenAiCompatible::new("http://localhost:11434/v1", "key").is_ok());
    }

    #[test]
    fn blank_keys_allowed_for_custom_endpoints() {
        let provider = OpenAiCompatible::with_options(
            "http://localhost:11434/v1",
            "",
            "custom.local",
            HeaderMap::new(),
            true,
            TemperaturePolicy::Passthrough,
        )
        .expect("blank key allowed");
        assert!(provider.allows_blank_key());
    }

    #[test]
    fn body_omits_unset_knobs() {
        let request = ChatRequest {
            model: "m".to_string(),
            messages: vec![ChatMessage::user("hi")],
            max_tokens: None,
            temperature: None,
        };
        let body = openai_body(
            &request,
            "openai",
            TemperaturePolicy::Passthrough,
            MaxTokensField::MaxTokens,
        );
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("temperature").is_none());
        assert_eq!(body.get("stream"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn body_respects_temperature_policy_and_token_field() {
        let request = ChatRequest {
            model: "m".to_string(),
            messages: vec![ChatMessage::user("hi")],
            max_tokens: Some(64),
            temperature: Some(0.2),
        };
        let body = openai_body(
            &request,
            "kimi",
            TemperaturePolicy::Omit,
            MaxTokensField::MaxTokens,
        );
        assert!(body.get("temperature").is_none());
        assert_eq!(body.get("max_tokens"), Some(&serde_json::json!(64)));

        let body = openai_body(
            &request,
            "openai",
            TemperaturePolicy::Passthrough,
            MaxTokensField::MaxCompletionTokens,
        );
        assert_eq!(
            body.get("max_completion_tokens"),
            Some(&serde_json::json!(64))
        );
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn model_lists_parse_every_shape() {
        let openai = serde_json::json!({ "data": [{ "id": "b" }, { "id": "a" }] });
        assert_eq!(parse_model_ids(&openai), vec!["b", "a"]);
        let anthropic = serde_json::json!({ "data": [{ "id": "claude" }] });
        assert_eq!(parse_model_ids(&anthropic), vec!["claude"]);
        let bare = serde_json::json!(["x", "y"]);
        assert_eq!(parse_model_ids(&bare), vec!["x", "y"]);
        assert!(parse_model_ids(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn empty_chain_is_rejected() {
        assert!(AnyProvider::from_targets(Vec::new()).is_err());
    }

    #[test]
    fn chain_skips_unusable_entries() {
        let provider = AnyProvider::from_targets(vec![
            ProviderTarget {
                provider: "custom.no-url".to_string(),
                model: "m".to_string(),
                base_url: None,
                api_key: String::new(),
            },
            ProviderTarget {
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                base_url: Some("http://localhost:1/v1".to_string()),
                api_key: "test".to_string(),
            },
        ])
        .expect("one usable target");
        assert_eq!(provider.chain_len(), 1);
    }

    #[test]
    fn all_unusable_entries_report_each_failure() {
        let result = AnyProvider::from_targets(vec![ProviderTarget {
            provider: "custom.broken".to_string(),
            model: "m".to_string(),
            base_url: None,
            api_key: String::new(),
        }]);
        let err = match result {
            Ok(_) => panic!("no usable target expected"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("custom.broken"));
    }
}
