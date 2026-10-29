//! Shared LLM configuration: provider catalog, models, and the fallback chain.
//!
//! One place resolves "which providers, models, endpoints, and keys" so
//! research, compression, synthesis, skills, and the Settings self-test cannot
//! drift. Keys are read here and never returned over IPC.
//!
//! The configuration is stored as JSON in the flat `settings` table:
//!
//! * `llm.chain`            ordered `[{provider, model}]` fallback chain
//! * `llm.custom_providers` user-defined OpenAI-compatible endpoints
//! * `llm.provider_models`  per-provider model lists (synced or manual) as JSON
//! * `llm.base_urls`        per-provider endpoint overrides as JSON
//!
//! Secrets stay in the OS keychain under the provider id.

use std::collections::{BTreeMap, BTreeSet};

use el_core::{AppError, AppResult};
use el_llm::catalog::{self, ProviderDescriptor};
use el_llm::ProviderTarget;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// Ordered fallback chain.
pub const SETTING_CHAIN: &str = "llm.chain";
/// User-defined OpenAI-compatible endpoints.
pub const SETTING_CUSTOM_PROVIDERS: &str = "llm.custom_providers";
/// Per-provider model lists.
pub const SETTING_PROVIDER_MODELS: &str = "llm.provider_models";
/// Per-provider endpoint overrides.
pub const SETTING_BASE_URLS: &str = "llm.base_urls";
/// Legacy single-provider keys, kept in sync for older code paths.
pub const SETTING_LEGACY_PROVIDER: &str = "llm.provider";
pub const SETTING_LEGACY_MODEL: &str = "llm.model";
pub const SETTING_LEGACY_BASE_URL: &str = "llm.base_url";

/// Guard rails so a corrupted settings row cannot create an unbounded loop.
const MAX_CHAIN: usize = 20;
const MAX_CUSTOM_PROVIDERS: usize = 20;
const MAX_MODELS_PER_PROVIDER: usize = 200;

/// One entry in the fallback chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ChainEntry {
    pub provider: String,
    pub model: String,
}

/// A user-defined OpenAI-compatible endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CustomProvider {
    /// `custom.<slug>`; also the keychain account name.
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<String>,
}

/// Everything Settings needs to render and edit the model configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize, specta::Type)]
pub struct LlmConfig {
    pub chain: Vec<ChainEntry>,
    pub custom_providers: Vec<CustomProvider>,
    pub provider_models: BTreeMap<String, Vec<String>>,
    pub base_urls: BTreeMap<String, String>,
}

/// Default model per provider; the chain entry overrides it.
pub fn default_model_for(provider: &str) -> &'static str {
    catalog::default_model_for(provider)
}

/// Every built-in provider, for the Settings picker.
#[tauri::command]
#[specta::specta]
pub fn llm_catalog() -> AppResult<Vec<ProviderDescriptor>> {
    Ok(catalog::descriptors())
}

/// Reads the model configuration, translating legacy single-provider settings.
#[tauri::command]
#[specta::specta]
pub fn llm_config_get(state: State<'_, AppState>) -> AppResult<LlmConfig> {
    load_config(&state)
}

/// Validates and persists the model configuration.
#[tauri::command]
#[specta::specta]
pub fn llm_config_save(state: State<'_, AppState>, config: LlmConfig) -> AppResult<LlmConfig> {
    save_config(&state, config)
}

/// Fetches a provider's live model list from `/models` and merges it with the
/// stored list. The result is returned only; Settings saves it with the rest of
/// the configuration so a sync cannot silently overwrite unsaved edits.
#[tauri::command]
#[specta::specta]
pub async fn llm_models_sync(
    state: State<'_, AppState>,
    provider: String,
) -> AppResult<Vec<String>> {
    let mut config = load_config(&state)?;
    let (base_url, api_key) = endpoint_for(&state, &config, &provider)?;
    let fetched = el_llm::provider::list_models(&provider, &base_url, &api_key).await?;
    let stored = config.provider_models.entry(provider.clone()).or_default();
    merge_models(stored, fetched);
    Ok(config
        .provider_models
        .get(&provider)
        .cloned()
        .unwrap_or_default())
}

/// Reads the configuration from the settings table.
pub fn load_config(state: &AppState) -> AppResult<LlmConfig> {
    let conn = state.db.connect()?;
    load_config_from(&conn)
}

fn load_config_from(conn: &rusqlite::Connection) -> AppResult<LlmConfig> {
    let stored_chain: Option<Vec<ChainEntry>> = read_json(conn, SETTING_CHAIN)?;
    let mut config = LlmConfig {
        chain: stored_chain.clone().unwrap_or_default(),
        custom_providers: read_json(conn, SETTING_CUSTOM_PROVIDERS)?.unwrap_or_default(),
        provider_models: read_json(conn, SETTING_PROVIDER_MODELS)?.unwrap_or_default(),
        base_urls: read_json(conn, SETTING_BASE_URLS)?.unwrap_or_default(),
    };
    if stored_chain.is_none() {
        let (chain, migrated_custom) = legacy_chain(conn)?;
        config.chain = chain;
        if let Some(custom) = migrated_custom {
            if !config
                .custom_providers
                .iter()
                .any(|existing| existing.id == custom.id)
            {
                config.custom_providers.push(custom);
            }
        }
    }
    if config.chain.is_empty() {
        config.chain = vec![ChainEntry {
            provider: "openai".to_string(),
            model: default_model_for("openai").to_string(),
        }];
    }
    Ok(config)
}

/// Reproduces the pre-chain single-provider setup from the legacy keys.
///
/// The old "OpenAI + custom base URL" combination becomes a real custom
/// endpoint so the user keeps working without re-entering anything.
fn legacy_chain(
    conn: &rusqlite::Connection,
) -> AppResult<(Vec<ChainEntry>, Option<CustomProvider>)> {
    let provider = get_setting(conn, SETTING_LEGACY_PROVIDER)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "openai".to_string());
    let model = get_setting(conn, SETTING_LEGACY_MODEL)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_model_for(&provider).to_string());
    let legacy_base = get_setting(conn, SETTING_LEGACY_BASE_URL)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if provider == "openai" {
        if let Some(base_url) = legacy_base {
            if base_url != el_llm::OPENAI_BASE_URL {
                let custom = CustomProvider {
                    id: "custom.legacy".to_string(),
                    display_name: "Custom Endpoint (migrated)".to_string(),
                    base_url,
                    models: vec![model.clone()],
                };
                return Ok((
                    vec![ChainEntry {
                        provider: custom.id.clone(),
                        model,
                    }],
                    Some(custom),
                ));
            }
        }
    }
    Ok((vec![ChainEntry { provider, model }], None))
}

/// Writes the legacy base URL / single-provider keys into the chain format.
///
/// Runs once at startup, mirroring the legacy-key migration for secrets.
pub fn migrate_config(state: &AppState) -> AppResult<()> {
    let conn = state.db.connect()?;
    if get_setting(&conn, SETTING_CHAIN)?.is_some() {
        return Ok(());
    }
    let config = load_config_from(&conn)?;
    write_config(&conn, &config)?;
    // The legacy custom endpoint reused the OpenAI key. Carry it across so the
    // migrated endpoint keeps working without asking for the key again.
    for custom in &config.custom_providers {
        if custom.id != "custom.legacy" {
            continue;
        }
        let already_stored = el_security::has_api_key(&custom.id).unwrap_or(false);
        if !already_stored {
            if let Ok(key) = el_security::load_api_key("openai") {
                if let Err(err) = el_security::set_api_key(&custom.id, &key) {
                    tracing::warn!(%err, "could not copy the OpenAI key to the migrated custom endpoint");
                }
            }
        }
    }
    tracing::info!(
        chain_len = config.chain.len(),
        "migrated LLM settings to the fallback-chain format"
    );
    Ok(())
}

/// Validates and persists a configuration, deleting keys for removed custom
/// endpoints so the keychain does not accumulate orphans.
pub fn save_config(state: &AppState, mut config: LlmConfig) -> AppResult<LlmConfig> {
    normalize(&mut config);
    validate(&config)?;
    let previous = load_config(state)?;
    {
        let conn = state.db.connect()?;
        write_config(&conn, &config)?;
    }
    let kept: BTreeSet<&str> = config
        .custom_providers
        .iter()
        .map(|custom| custom.id.as_str())
        .collect();
    for custom in &previous.custom_providers {
        if !kept.contains(custom.id.as_str()) {
            if let Err(err) = el_security::delete_api_key(&custom.id) {
                tracing::warn!(provider = %custom.id, %err, "could not delete removed endpoint key");
            }
        }
    }
    Ok(config)
}

fn write_config(conn: &rusqlite::Connection, config: &LlmConfig) -> AppResult<()> {
    set_json(conn, SETTING_CHAIN, &config.chain)?;
    set_json(conn, SETTING_CUSTOM_PROVIDERS, &config.custom_providers)?;
    set_json(conn, SETTING_PROVIDER_MODELS, &config.provider_models)?;
    set_json(conn, SETTING_BASE_URLS, &config.base_urls)?;
    // Keep the legacy keys pointing at the primary chain entry so older code
    // paths and downgrades still resolve something sensible.
    if let Some(primary) = config.chain.first() {
        el_db::repo::cache::set_setting(conn, SETTING_LEGACY_PROVIDER, &primary.provider)?;
        el_db::repo::cache::set_setting(conn, SETTING_LEGACY_MODEL, &primary.model)?;
    }
    Ok(())
}

/// Builds the runtime provider chain from settings plus the keychain.
///
/// Entries without a usable key are skipped rather than failing the whole
/// chain: a user with Anthropic as a fallback but no Anthropic key should still
/// get OpenAI.
pub fn build_provider(state: &AppState) -> AppResult<el_llm::AnyProvider> {
    let config = load_config(state)?;
    let targets = resolve_targets(state, &config)?;
    el_llm::AnyProvider::from_targets(targets)
}

/// Resolves the configured chain into providers, skipping unusable entries.
pub fn resolve_targets(_state: &AppState, config: &LlmConfig) -> AppResult<Vec<ProviderTarget>> {
    let mut targets = Vec::new();
    let mut problems = Vec::new();
    for entry in &config.chain {
        let custom = config
            .custom_providers
            .iter()
            .find(|custom| custom.id == entry.provider);
        if catalog::is_custom(&entry.provider) && custom.is_none() {
            problems.push(format!(
                "{}: custom endpoint is no longer defined",
                entry.provider
            ));
            continue;
        }
        let base_url = config
            .base_urls
            .get(&entry.provider)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| custom.map(|custom| custom.base_url.clone()))
            .or_else(|| {
                let default = catalog::default_base_url_for(&entry.provider);
                (!default.is_empty()).then(|| default.to_string())
            });
        let api_key = match el_security::load_api_key(&entry.provider) {
            Ok(key) => key,
            Err(err) if custom.is_some() => {
                // Local OpenAI-compatible servers usually have no key.
                let _ = err;
                String::new()
            }
            Err(err) => {
                problems.push(format!("{}: {err}", entry.provider));
                continue;
            }
        };
        targets.push(ProviderTarget {
            provider: entry.provider.clone(),
            model: entry.model.clone(),
            base_url,
            api_key,
        });
    }
    if targets.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "no provider in the model chain is usable: {}",
            problems.join("; ")
        )));
    }
    Ok(targets)
}

/// Reads the configured primary model, falling back to the catalog default.
pub fn configured_model(state: &AppState) -> AppResult<String> {
    let config = load_config(state)?;
    if let Some(primary) = config.chain.first() {
        if !primary.model.trim().is_empty() {
            return Ok(primary.model.clone());
        }
        return Ok(default_model_for(&primary.provider).to_string());
    }
    let conn = state.db.connect()?;
    let provider = get_setting(&conn, SETTING_LEGACY_PROVIDER)?.unwrap_or_default();
    Ok(default_model_for(&provider).to_string())
}

/// Resolves the endpoint and key for one provider, for model sync and tests.
pub fn endpoint_for(
    state: &AppState,
    config: &LlmConfig,
    provider: &str,
) -> AppResult<(String, String)> {
    let custom = config
        .custom_providers
        .iter()
        .find(|custom| custom.id == provider);
    let base_url = config
        .base_urls
        .get(provider)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| custom.map(|custom| custom.base_url.clone()))
        .or_else(|| {
            let default = catalog::default_base_url_for(provider);
            (!default.is_empty()).then(|| default.to_string())
        })
        .ok_or_else(|| {
            AppError::InvalidInput(format!(
                "no base URL configured for {provider}; add one in Settings"
            ))
        })?;
    let api_key = match el_security::load_api_key(provider) {
        Ok(key) => key,
        Err(_) if custom.is_some() => String::new(),
        Err(err) => return Err(err),
    };
    let _ = state;
    Ok((base_url, api_key))
}

/// Trims text, removes blanks and duplicates, and caps the collection sizes.
fn normalize(config: &mut LlmConfig) {
    config.chain.truncate(MAX_CHAIN);
    for entry in &mut config.chain {
        entry.provider = entry.provider.trim().to_string();
        entry.model = entry.model.trim().to_string();
    }
    config.chain.retain(|entry| !entry.provider.is_empty());
    for custom in &mut config.custom_providers {
        custom.id = custom.id.trim().to_lowercase();
        custom.display_name = custom.display_name.trim().to_string();
        custom.base_url = custom.base_url.trim().trim_end_matches('/').to_string();
        clean_models(&mut custom.models);
    }
    config.custom_providers.truncate(MAX_CUSTOM_PROVIDERS);
    for models in config.provider_models.values_mut() {
        clean_models(models);
    }
    config.base_urls.retain(|_, value| !value.trim().is_empty());
    for value in config.base_urls.values_mut() {
        *value = value.trim().trim_end_matches('/').to_string();
    }
}

fn clean_models(models: &mut Vec<String>) {
    for model in models.iter_mut() {
        *model = model.trim().to_string();
    }
    models.retain(|model| !model.is_empty());
    models.sort();
    models.dedup();
    models.truncate(MAX_MODELS_PER_PROVIDER);
}

fn merge_models(stored: &mut Vec<String>, fetched: Vec<String>) {
    stored.extend(fetched);
    clean_models(stored);
}

/// Validates a configuration before it is persisted.
fn validate(config: &LlmConfig) -> AppResult<()> {
    if config.chain.is_empty() {
        return Err(AppError::InvalidInput(
            "the model chain needs at least one model".to_string(),
        ));
    }
    let custom_ids: BTreeSet<&str> = config
        .custom_providers
        .iter()
        .map(|custom| custom.id.as_str())
        .collect();
    if custom_ids.len() != config.custom_providers.len() {
        return Err(AppError::InvalidInput(
            "custom endpoint ids must be unique".to_string(),
        ));
    }
    for custom in &config.custom_providers {
        catalog::validate_custom_id(&custom.id)?;
        if custom.display_name.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "custom endpoint {} needs a display name",
                custom.id
            )));
        }
        validate_url(&custom.base_url)?;
    }
    for (provider, url) in &config.base_urls {
        ensure_provider_known(config, provider)?;
        validate_url(url)?;
    }
    for (provider, models) in &config.provider_models {
        ensure_provider_known(config, provider)?;
        if models.len() > MAX_MODELS_PER_PROVIDER {
            return Err(AppError::InvalidInput(format!(
                "{provider} has more than {MAX_MODELS_PER_PROVIDER} models"
            )));
        }
    }
    for entry in &config.chain {
        ensure_provider_known(config, &entry.provider)?;
        if entry.model.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "choose a model for {}",
                entry.provider
            )));
        }
    }
    Ok(())
}

fn ensure_provider_known(config: &LlmConfig, provider: &str) -> AppResult<()> {
    if catalog::lookup(provider).is_some()
        || config
            .custom_providers
            .iter()
            .any(|custom| custom.id == provider)
    {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "unknown provider {provider:?}; built-ins and configured custom endpoints only"
        )))
    }
}

fn validate_url(url: &str) -> AppResult<()> {
    let trimmed = url.trim();
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "provider URL must start with http:// or https://, got {url:?}"
        )))
    }
}

fn read_json<T: DeserializeOwned>(conn: &rusqlite::Connection, key: &str) -> AppResult<Option<T>> {
    match get_setting(conn, key)? {
        Some(raw) => serde_json::from_str(&raw).map(Some).map_err(|err| {
            AppError::InvalidInput(format!("setting {key} is not valid JSON: {err}"))
        }),
        None => Ok(None),
    }
}

fn set_json<T: Serialize>(conn: &rusqlite::Connection, key: &str, value: &T) -> AppResult<()> {
    let encoded = serde_json::to_string(value)
        .map_err(|err| AppError::Internal(format!("serialize {key}: {err}")))?;
    el_db::repo::cache::set_setting(conn, key, &encoded)
}

fn get_setting(conn: &rusqlite::Connection, key: &str) -> AppResult<Option<String>> {
    el_db::repo::cache::get_setting(conn, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(chain: Vec<ChainEntry>) -> LlmConfig {
        LlmConfig {
            chain,
            ..LlmConfig::default()
        }
    }

    #[test]
    fn rejects_empty_chain() {
        assert!(validate(&config_with(Vec::new())).is_err());
    }

    #[test]
    fn rejects_unknown_provider() {
        assert!(validate(&config_with(vec![ChainEntry {
            provider: "sky-net".to_string(),
            model: "x".to_string(),
        }]))
        .is_err());
    }

    #[test]
    fn accepts_known_and_custom_providers() {
        let mut config = config_with(vec![
            ChainEntry {
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
            },
            ChainEntry {
                provider: "custom.local".to_string(),
                model: "llama3".to_string(),
            },
        ]);
        config.custom_providers.push(CustomProvider {
            id: "custom.local".to_string(),
            display_name: "Local".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            models: vec!["llama3".to_string()],
        });
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn rejects_duplicate_custom_ids() {
        let mut config = config_with(vec![ChainEntry {
            provider: "custom.local".to_string(),
            model: "m".to_string(),
        }]);
        for _ in 0..2 {
            config.custom_providers.push(CustomProvider {
                id: "custom.local".to_string(),
                display_name: "Local".to_string(),
                base_url: "http://localhost:11434/v1".to_string(),
                models: vec!["m".to_string()],
            });
        }
        assert!(validate(&config).is_err());
    }

    #[test]
    fn rejects_non_http_urls() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("https://api.example.com").is_ok());
        assert!(validate_url("http://localhost:8080/v1").is_ok());
    }

    #[test]
    fn normalize_cleans_models_and_urls() {
        let mut config = config_with(vec![ChainEntry {
            provider: "openai".to_string(),
            model: "  gpt-4o-mini ".to_string(),
        }]);
        config.provider_models.insert(
            "openai".to_string(),
            vec![" b ".into(), "a".into(), "a".into(), "".into()],
        );
        config.base_urls.insert(
            "openai".to_string(),
            "https://proxy.example.com/v1/".to_string(),
        );
        normalize(&mut config);
        assert_eq!(config.chain[0].model, "gpt-4o-mini");
        assert_eq!(
            config.provider_models.get("openai"),
            Some(&vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            config.base_urls.get("openai").map(String::as_str),
            Some("https://proxy.example.com/v1")
        );
    }

    #[test]
    fn merge_models_sorts_and_dedupes() {
        let mut stored = vec!["b".to_string(), "a".to_string()];
        merge_models(&mut stored, vec!["a".to_string(), "c".to_string()]);
        assert_eq!(stored, vec!["a", "b", "c"]);
    }
}
