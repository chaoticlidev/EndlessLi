//! Static catalog of every built-in LLM provider.
//!
//! Provider identity used to be duplicated across the keychain allow-list, the
//! Settings dropdown, the Tauri default-model table, and the self-test match.
//! This module is the single source of truth: the keywords, wire dialect,
//! default endpoint, suggested models, and auth requirements live here, and
//! every other layer derives from it.
//!
//! Providers are grouped by wire dialect rather than by company. Almost every
//! vendor exposes an OpenAI-compatible `chat/completions` surface, so one
//! adapter covers them; Anthropic keeps its Messages dialect and GitHub Copilot
//! keeps its token exchange.

use el_core::{AppError, AppResult};

/// HTTP dialect a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WireFormat {
    /// OpenAI `chat/completions` with SSE streaming.
    OpenAi,
    /// Anthropic Messages with SSE event names.
    Anthropic,
    /// GitHub's Copilot endpoint: OpenAI-compatible after a token exchange.
    Copilot,
}

/// Name of the output-token cap field a provider expects.
///
/// OpenAI deprecated `max_tokens` for its newer reasoning models in favour of
/// `max_completion_tokens`; OpenAI-compatible vendors still expect `max_tokens`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxTokensField {
    MaxTokens,
    MaxCompletionTokens,
}

/// How a provider treats the `temperature` field on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemperaturePolicy {
    /// Forward the caller's value when one is set.
    Passthrough,
    /// Never send `temperature`; some models only accept their own default.
    Omit,
}

/// One compile-time provider description.
///
/// Kept as borrowed strings so the whole catalog can live in a `static` slice
/// with no allocation; [`ProviderDescriptor`] is the owned copy sent to the UI.
pub struct CatalogEntry {
    /// Stable id stored in settings, the keychain, and usage rows.
    pub id: &'static str,
    /// Human-facing name.
    pub label: &'static str,
    /// Wire dialect.
    pub wire: WireFormat,
    /// Default API root (without the trailing `/chat/completions`).
    pub default_base_url: &'static str,
    /// Whether the user may override the endpoint.
    pub base_url_editable: bool,
    /// Model used when the user has not picked one.
    pub default_model: &'static str,
    /// Suggested models, shown before the user syncs or adds their own.
    pub models: &'static [&'static str],
    /// Whether the provider rejects requests without a credential.
    pub requires_key: bool,
    /// Console/documentation link shown as a hint.
    pub docs_url: &'static str,
    /// Extra guidance shown under the base-URL field.
    pub base_url_hint: &'static str,
    /// Temperature handling for this dialect.
    pub temperature: TemperaturePolicy,
}

impl CatalogEntry {
    /// Owned copy for IPC.
    pub fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: self.id.to_string(),
            label: self.label.to_string(),
            wire: self.wire,
            default_base_url: self.default_base_url.to_string(),
            base_url_editable: self.base_url_editable,
            default_model: self.default_model.to_string(),
            models: self.models.iter().map(|model| model.to_string()).collect(),
            requires_key: self.requires_key,
            docs_url: self.docs_url.to_string(),
            base_url_hint: self.base_url_hint.to_string(),
        }
    }
}

/// Provider description returned through IPC.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ProviderDescriptor {
    pub id: String,
    pub label: String,
    pub wire: WireFormat,
    pub default_base_url: String,
    pub base_url_editable: bool,
    pub default_model: String,
    pub models: Vec<String>,
    pub requires_key: bool,
    pub docs_url: String,
    pub base_url_hint: String,
}

/// Every built-in provider EndlessLi can talk to.
///
/// Order is the order shown in Settings. Model lists are suggestions: users can
/// sync the live catalog from `/models` or add any id by hand.
pub static CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "openai",
        label: "OpenAI",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.openai.com/v1",
        base_url_editable: true,
        default_model: "gpt-4o-mini",
        models: &["gpt-4o-mini", "gpt-4.1", "gpt-4.1-mini", "gpt-5.5"],
        requires_key: true,
        docs_url: "https://platform.openai.com/api-keys",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "anthropic",
        label: "Anthropic",
        wire: WireFormat::Anthropic,
        default_base_url: "https://api.anthropic.com/v1",
        base_url_editable: true,
        default_model: "claude-sonnet-4-5",
        models: &["claude-sonnet-4-5", "claude-opus-4-1", "claude-haiku-4-5"],
        requires_key: true,
        docs_url: "https://console.anthropic.com/settings/keys",
        base_url_hint: "Override only for a proxy that speaks the Anthropic Messages API.",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "google",
        label: "Google (Gemini)",
        wire: WireFormat::OpenAi,
        default_base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        base_url_editable: true,
        default_model: "gemini-2.5-flash",
        models: &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-3.8-flash"],
        requires_key: true,
        docs_url: "https://aistudio.google.com/app/apikey",
        base_url_hint: "Uses Gemini's OpenAI-compatible endpoint.",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "deepseek",
        label: "DeepSeek",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.deepseek.com/v1",
        base_url_editable: true,
        default_model: "deepseek-chat",
        models: &["deepseek-chat", "deepseek-reasoner"],
        requires_key: true,
        docs_url: "https://platform.deepseek.com/api_keys",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "groq",
        label: "Groq",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.groq.com/openai/v1",
        base_url_editable: true,
        default_model: "llama-3.3-70b-versatile",
        models: &[
            "llama-3.3-70b-versatile",
            "openai/gpt-oss-120b",
            "moonshotai/kimi-k2-instruct",
        ],
        requires_key: true,
        docs_url: "https://console.groq.com/keys",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "cerebras",
        label: "Cerebras",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.cerebras.ai/v1",
        base_url_editable: true,
        default_model: "llama-3.3-70b",
        models: &["llama-3.3-70b", "gpt-oss-120b", "qwen-3-32b"],
        requires_key: true,
        docs_url: "https://cloud.cerebras.ai",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "openrouter",
        label: "OpenRouter",
        wire: WireFormat::OpenAi,
        default_base_url: "https://openrouter.ai/api/v1",
        base_url_editable: true,
        default_model: "openai/gpt-4o-mini",
        models: &[
            "openai/gpt-4o-mini",
            "anthropic/claude-sonnet-4.5",
            "google/gemini-2.5-pro",
            "deepseek/deepseek-chat",
        ],
        requires_key: true,
        docs_url: "https://openrouter.ai/keys",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "qwen",
        label: "Qwen (DashScope)",
        wire: WireFormat::OpenAi,
        default_base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        base_url_editable: true,
        default_model: "qwen3-max",
        models: &["qwen3-max", "qwen-plus", "qwen3-coder-plus"],
        requires_key: true,
        docs_url: "https://bailian.console.alibabacloud.com",
        base_url_hint: "China accounts use https://dashscope.aliyuncs.com/compatible-mode/v1.",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "kimi",
        label: "Kimi (Moonshot)",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.moonshot.ai/v1",
        base_url_editable: true,
        default_model: "kimi-k2.6",
        models: &["kimi-k2.6", "kimi-k2.7-code", "kimi-k3"],
        requires_key: true,
        docs_url: "https://platform.kimi.ai/console/api-keys",
        base_url_hint: "China accounts use https://api.moonshot.cn/v1. Some K2 models only accept temperature 1, so it is omitted.",
        temperature: TemperaturePolicy::Omit,
    },
    CatalogEntry {
        id: "minimax",
        label: "MiniMax",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.minimax.io/v1",
        base_url_editable: true,
        default_model: "MiniMax-M3.1-Flash",
        models: &["MiniMax-M3.1-Flash", "MiniMax-M3", "MiniMax-M2.7"],
        requires_key: true,
        docs_url: "https://platform.minimax.io/user-center/basic-information/interface-key",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "nvidia",
        label: "NVIDIA NIM",
        wire: WireFormat::OpenAi,
        default_base_url: "https://integrate.api.nvidia.com/v1",
        base_url_editable: true,
        default_model: "meta/llama-3.3-70b-instruct",
        models: &[
            "meta/llama-3.3-70b-instruct",
            "deepseek-ai/deepseek-r1",
            "nvidia/llama-3.1-nemotron-70b-instruct",
        ],
        requires_key: true,
        docs_url: "https://build.nvidia.com",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "fireworks",
        label: "Fireworks",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.fireworks.ai/inference/v1",
        base_url_editable: true,
        default_model: "accounts/fireworks/models/llama-v3p3-70b-instruct",
        models: &[
            "accounts/fireworks/models/llama-v3p3-70b-instruct",
            "accounts/fireworks/models/deepseek-v3",
        ],
        requires_key: true,
        docs_url: "https://fireworks.ai/account/api-keys",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "huggingface",
        label: "HuggingFace",
        wire: WireFormat::OpenAi,
        default_base_url: "https://router.huggingface.co/v1",
        base_url_editable: true,
        default_model: "openai/gpt-oss-120b",
        models: &[
            "openai/gpt-oss-120b",
            "Qwen/Qwen3-235B-A22B-Instruct-2507",
            "moonshotai/Kimi-K2-Instruct",
        ],
        requires_key: true,
        docs_url: "https://huggingface.co/settings/tokens",
        base_url_hint: "Add a provider suffix such as :fastest to route explicitly.",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "meta",
        label: "Meta (Llama API)",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.llama.com/compat/v1",
        base_url_editable: true,
        default_model: "Llama-4-Maverick-17B-128E-Instruct-FP8",
        models: &[
            "Llama-4-Maverick-17B-128E-Instruct-FP8",
            "Llama-4-Scout-17B-16E-Instruct-FP8",
            "Llama-3.3-8B-Instruct",
        ],
        requires_key: true,
        docs_url: "https://llama.developer.meta.com",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "zai",
        label: "Z.ai",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.z.ai/api/paas/v4",
        base_url_editable: true,
        default_model: "glm-5.3",
        models: &["glm-5.3", "glm-5.2", "glm-4.6"],
        requires_key: true,
        docs_url: "https://z.ai/manage-apikey/apikey-list",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "ant-ling",
        label: "Ant Ling",
        wire: WireFormat::OpenAi,
        default_base_url: "https://api.ant-ling.com/v1",
        base_url_editable: true,
        default_model: "Ling-3.1-flash",
        models: &[
            "Ling-3.1-flash",
            "Ling-3.0-flash",
            "Ling-3.0-flash-VL",
            "Ling-3.0-tiny",
        ],
        requires_key: true,
        docs_url: "https://chat.ant-ling.com/open",
        base_url_hint: "Model ids are case-sensitive.",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "opencode-zen",
        label: "OpenCode Zen",
        wire: WireFormat::OpenAi,
        default_base_url: "https://opencode.ai/zen/v1",
        base_url_editable: true,
        default_model: "claude-sonnet-4-5",
        models: &[
            "claude-sonnet-4-5",
            "qwen3.8-max",
            "kimi-k2.6",
            "glm-5.3",
            "gpt-5.5",
        ],
        requires_key: true,
        docs_url: "https://opencode.ai/auth",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "vercel",
        label: "Vercel AI Gateway",
        wire: WireFormat::OpenAi,
        default_base_url: "https://ai-gateway.vercel.sh/v1",
        base_url_editable: true,
        default_model: "openai/gpt-4.1",
        models: &[
            "openai/gpt-4.1",
            "anthropic/claude-sonnet-4.5",
            "google/gemini-2.5-pro",
        ],
        requires_key: true,
        docs_url: "https://vercel.com/dashboard/ai-gateway/api-keys",
        base_url_hint: "",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "cloudflare",
        label: "Cloudflare AI Gateway",
        wire: WireFormat::OpenAi,
        default_base_url: "https://gateway.ai.cloudflare.com/v1/{account_id}/{gateway_id}/compat",
        base_url_editable: true,
        default_model: "openai/gpt-4.1",
        models: &[
            "openai/gpt-4.1",
            "anthropic/claude-4-5-sonnet",
            "@cf/meta/llama-3.3-70b-instruct-fp8-fast",
        ],
        requires_key: true,
        docs_url: "https://dash.cloudflare.com",
        base_url_hint: "Replace {account_id} and {gateway_id}. Cloudflare's unified endpoint is /compat.",
        temperature: TemperaturePolicy::Passthrough,
    },
    CatalogEntry {
        id: "github-copilot",
        label: "GitHub Copilot",
        wire: WireFormat::Copilot,
        default_base_url: "https://api.githubcopilot.com",
        base_url_editable: true,
        default_model: "gpt-4o",
        models: &["gpt-4o", "claude-3.5-sonnet", "o3-mini"],
        requires_key: true,
        docs_url: "https://github.com/settings/tokens",
        base_url_hint: "Use a GitHub token (ghu_/ghp_/github_pat_) or a Copilot API token; GitHub tokens are exchanged automatically.",
        temperature: TemperaturePolicy::Passthrough,
    },
];

/// Prefix reserved for user-defined OpenAI-compatible endpoints.
pub const CUSTOM_PREFIX: &str = "custom.";

/// Looks up a built-in provider.
pub fn lookup(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

/// Every built-in provider as an owned descriptor.
pub fn descriptors() -> Vec<ProviderDescriptor> {
    CATALOG.iter().map(CatalogEntry::descriptor).collect()
}

/// Default model for a provider id, falling back to the first catalog entry.
pub fn default_model_for(id: &str) -> &'static str {
    lookup(id)
        .map(|entry| entry.default_model)
        .filter(|model| !model.is_empty())
        .unwrap_or("gpt-4o-mini")
}

/// Default base URL for a provider id, or an empty string when the provider
/// has no fixed endpoint (a custom endpoint must supply one).
pub fn default_base_url_for(id: &str) -> &'static str {
    lookup(id).map(|entry| entry.default_base_url).unwrap_or("")
}

/// Wire dialect for a provider id. Unknown ids are treated as OpenAI-compatible
/// so a user's endpoint keeps working even if the catalog changes.
pub fn wire_for(id: &str) -> WireFormat {
    lookup(id)
        .map(|entry| entry.wire)
        .unwrap_or(WireFormat::OpenAi)
}

/// Temperature policy for a provider id.
pub fn temperature_for(id: &str) -> TemperaturePolicy {
    lookup(id)
        .map(|entry| entry.temperature)
        .unwrap_or(TemperaturePolicy::Passthrough)
}

/// Which output-token field the provider expects.
pub fn max_tokens_field_for(id: &str) -> MaxTokensField {
    match id {
        // OpenAI's reasoning models reject `max_tokens`.
        "openai" => MaxTokensField::MaxCompletionTokens,
        _ => MaxTokensField::MaxTokens,
    }
}

/// Whether the provider id is a user-defined endpoint.
pub fn is_custom(id: &str) -> bool {
    id.starts_with(CUSTOM_PREFIX)
}

/// Validates a user-supplied provider id for a custom endpoint.
///
/// The id becomes a keychain account and a settings key, so it is restricted to
/// lowercase letters, digits, and dashes after the `custom.` prefix. This blocks
/// path-like or arbitrary strings from reaching the OS keychain.
pub fn validate_custom_id(id: &str) -> AppResult<()> {
    let slug = id.strip_prefix(CUSTOM_PREFIX).unwrap_or("");
    let valid = !slug.is_empty()
        && slug.len() <= 40
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-');
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "invalid custom provider id {id:?}; expected custom.<lowercase-letters-digits-dashes> (40 chars max)"
        )))
    }
}

/// Builds a safe provider id from a display name.
pub fn custom_id_from_name(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-');
    let mut compact = String::new();
    for ch in trimmed.chars() {
        if ch == '-' && compact.ends_with('-') {
            continue;
        }
        compact.push(ch);
    }
    let compact = compact.chars().take(40).collect::<String>();
    let compact = compact.trim_matches('-');
    if compact.is_empty() {
        format!("{CUSTOM_PREFIX}endpoint")
    } else {
        format!("{CUSTOM_PREFIX}{compact}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_https() {
        let mut seen = std::collections::HashSet::new();
        for entry in CATALOG {
            assert!(seen.insert(entry.id), "duplicate provider id {}", entry.id);
            assert!(
                entry.default_base_url.is_empty() || entry.default_base_url.starts_with("https://"),
                "{} must use https",
                entry.id
            );
            assert!(
                !entry.default_model.is_empty(),
                "{} needs a default model",
                entry.id
            );
        }
        assert!(CATALOG.len() >= 19, "expected all requested providers");
    }

    #[test]
    fn lookups_match_ids() {
        assert_eq!(default_model_for("anthropic"), "claude-sonnet-4-5");
        assert_eq!(wire_for("anthropic"), WireFormat::Anthropic);
        assert_eq!(wire_for("github-copilot"), WireFormat::Copilot);
        assert_eq!(wire_for("custom.mine"), WireFormat::OpenAi);
        assert!(lookup("nope").is_none());
    }

    #[test]
    fn custom_ids_are_validated() {
        assert!(validate_custom_id("custom.local-llama").is_ok());
        assert!(validate_custom_id("custom.").is_err());
        assert!(validate_custom_id("custom.Upper").is_err());
        assert!(validate_custom_id("custom.a/b").is_err());
        assert!(validate_custom_id("openai").is_err());
    }

    #[test]
    fn ids_generated_from_names_are_safe() {
        assert_eq!(
            custom_id_from_name("My Local Server!"),
            "custom.my-local-server"
        );
        assert_eq!(custom_id_from_name("  "), "custom.endpoint");
        assert!(validate_custom_id(&custom_id_from_name("A__B")).is_ok());
    }

    #[test]
    fn descriptors_copy_model_lists() {
        let anthropic = lookup("anthropic").expect("anthropic").descriptor();
        assert_eq!(anthropic.id, "anthropic");
        assert!(anthropic.models.contains(&"claude-sonnet-4-5".to_string()));
        assert!(anthropic.requires_key);
    }
}
