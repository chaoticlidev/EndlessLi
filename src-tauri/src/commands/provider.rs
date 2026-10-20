//! Shared LLM provider construction from settings plus the keychain.
//!
//! One place resolves "which provider, base URL, model, and key" so research,
//! compression, and skills cannot drift. Keys are read here and never returned
//! over IPC.

use el_core::AppResult;

use crate::state::AppState;

/// Sensible default model per provider; the `llm.model` setting overrides it.
pub fn default_model_for(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-sonnet-4-5",
        "deepseek" => "deepseek-chat",
        _ => "gpt-4o-mini",
    }
}

/// Builds the configured provider from settings and the stored key.
pub fn build_provider(state: &AppState) -> AppResult<el_llm::AnyProvider> {
    let conn = state.db.connect()?;
    let provider = el_db::repo::cache::get_setting(&conn, "llm.provider")?
        .unwrap_or_else(|| "openai".to_string());
    let base_url = el_db::repo::cache::get_setting(&conn, "llm.base_url")?;
    let key = crate::commands::secrets::load_api_key(state, &provider)?;
    el_llm::AnyProvider::from_parts(&provider, base_url.as_deref(), &key)
}

/// Reads the configured model, falling back to the provider's default when the
/// `llm.model` setting is unset.
pub fn configured_model(state: &AppState) -> AppResult<String> {
    let conn = state.db.connect()?;
    if let Some(model) = el_db::repo::cache::get_setting(&conn, "llm.model")?
        .filter(|model| !model.trim().is_empty())
    {
        return Ok(model);
    }
    let provider = el_db::repo::cache::get_setting(&conn, "llm.provider")?.unwrap_or_default();
    Ok(default_model_for(&provider).to_string())
}
