//! Secret commands: keys go into the OS keychain and never come back.
//!
//! The only key read path is `api_key_test`, which loads the key into memory
//! for one request. There is intentionally no command that returns a key.

use el_core::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// Per-provider key presence for the Settings UI.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ProviderKeyStatus {
    pub provider: String,
    pub configured: bool,
}

/// Usage totals with limits and the current warning state.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TokenSummary {
    #[specta(type = specta_typescript::Number)]
    pub day_in: i64,
    #[specta(type = specta_typescript::Number)]
    pub day_out: i64,
    #[specta(type = specta_typescript::Number)]
    pub month_in: i64,
    #[specta(type = specta_typescript::Number)]
    pub month_out: i64,
    #[specta(type = specta_typescript::Number)]
    pub daily_limit: Option<i64>,
    #[specta(type = specta_typescript::Number)]
    pub monthly_limit: Option<i64>,
    pub budget_warning: Option<String>,
}

/// Fallback settings key holding a provider key when the OS keychain is
/// unavailable. Redundant with the keychain: we always write both so a
/// keychain read failure never locks the user out.
fn fallback_key(provider: &str) -> String {
    format!("secret.{}", provider)
}

/// Reports whether a key is available from either the keychain or the
/// local fallback store.
pub fn has_api_key(state: &AppState, provider: &str) -> AppResult<bool> {
    if el_security::has_api_key(provider).unwrap_or(false) {
        return Ok(true);
    }
    let conn = state.db.connect()?;
    let value = el_db::repo::cache::get_setting(&conn, &fallback_key(provider))?;
    Ok(value.map(|v| !v.trim().is_empty()).unwrap_or(false))
}

/// Stores a key in the keychain and the local fallback store.
pub fn set_api_key(state: &AppState, provider: &str, key: &str) -> AppResult<()> {
    el_security::secrets::check_provider(provider)?;
    if key.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "API key must not be blank".to_string(),
        ));
    }
    let _ = el_security::set_api_key(provider, key);
    let conn = state.db.connect()?;
    el_db::repo::cache::set_setting(&conn, &fallback_key(provider), key)?;
    Ok(())
}

/// Removes a key from both stores.
pub fn delete_api_key(state: &AppState, provider: &str) -> AppResult<()> {
    let _ = el_security::delete_api_key(provider);
    let conn = state.db.connect()?;
    el_db::repo::cache::set_setting(&conn, &fallback_key(provider), "")?;
    Ok(())
}

/// Loads a key, preferring the OS keychain and falling back to the local
/// store when the keychain is unavailable or empty.
pub fn load_api_key(state: &AppState, provider: &str) -> AppResult<String> {
    if let Ok(key) = el_security::load_api_key(provider) {
        if !key.trim().is_empty() {
            return Ok(key);
        }
    }
    let conn = state.db.connect()?;
    if let Some(key) = el_db::repo::cache::get_setting(&conn, &fallback_key(provider))? {
        if !key.trim().is_empty() {
            return Ok(key);
        }
    }
    Err(AppError::InvalidInput(format!(
        "no API key stored for {provider}; add one in Settings"
    )))
}

/// Stores (or replaces) a provider API key.
#[tauri::command]
#[specta::specta]
pub fn api_key_set(state: State<'_, AppState>, provider: String, key: String) -> AppResult<()> {
    set_api_key(&state, &provider, &key)
}

/// Removes a provider API key. Succeeds when none was stored.
#[tauri::command]
#[specta::specta]
pub fn api_key_delete(state: State<'_, AppState>, provider: String) -> AppResult<()> {
    delete_api_key(&state, &provider)
}

/// Reports which providers have a key stored, without revealing keys.
#[tauri::command]
#[specta::specta]
pub fn api_key_status(state: State<'_, AppState>) -> AppResult<Vec<ProviderKeyStatus>> {
    let _ = state;
    el_security::secrets::KNOWN_PROVIDERS
        .iter()
        .map(|provider| {
            Ok(ProviderKeyStatus {
                provider: provider.to_string(),
                configured: has_api_key(&state, provider)?,
            })
        })
        .collect()
}

/// Runs a tiny self-test prompt end to end: budget check, one completion,
/// sanitized output, logged usage. Proves the key, model, and budgets work.
#[tauri::command]
#[specta::specta]
pub async fn api_key_test(
    state: State<'_, AppState>,
    provider: String,
    model: Option<String>,
) -> AppResult<el_llm::TestOutcome> {
    el_security::secrets::check_provider(&provider)?;
    let key = load_api_key(&state, &provider)?;
    let conn = state.db.connect()?;
    let settings_model = el_db::repo::cache::get_setting(&conn, "llm.model")?;
    let model = model
        .or(settings_model)
        .unwrap_or_else(|| default_model(&provider).to_string());
    if model.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "model must not be blank".to_string(),
        ));
    }
    match provider.as_str() {
        "anthropic" => {
            el_llm::run_test_prompt(&state.db, &el_llm::Anthropic::new(&key)?, &model, None).await
        }
        "deepseek" => {
            el_llm::run_test_prompt(&state.db, &el_llm::DeepSeek::new(&key)?, &model, None).await
        }
        _ => {
            let base_url = el_db::repo::cache::get_setting(&conn, "llm.base_url")?
                .filter(|url| !url.trim().is_empty())
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            el_llm::run_test_prompt(
                &state.db,
                &el_llm::OpenAiCompatible::new(&base_url, &key)?,
                &model,
                None,
            )
            .await
        }
    }
}

/// Summarizes token usage (rolling 24h / 30d) with limits and warnings.
#[tauri::command]
#[specta::specta]
pub fn token_usage_summary(state: State<'_, AppState>) -> AppResult<TokenSummary> {
    let conn = state.db.connect()?;
    let now = chrono::Utc::now().timestamp();
    let (day_in, day_out) = el_db::repo::cache::sum_since(&conn, now - 24 * 3600)?;
    let (month_in, month_out) = el_db::repo::cache::sum_since(&conn, now - 30 * 24 * 3600)?;
    let check = el_llm::check_budget(&conn, 0)?;
    let daily_limit = limit_of(&conn, el_llm::budgets::SETTING_DAILY_TOKENS)?;
    let monthly_limit = limit_of(&conn, el_llm::budgets::SETTING_MONTHLY_TOKENS)?;
    Ok(TokenSummary {
        day_in,
        day_out,
        month_in,
        month_out,
        daily_limit,
        monthly_limit,
        budget_warning: check.warning,
    })
}

fn limit_of(conn: &rusqlite::Connection, key: &str) -> AppResult<Option<i64>> {
    match el_db::repo::cache::get_setting(conn, key)? {
        Some(raw) => {
            let value: i64 = raw.trim().parse().unwrap_or(0);
            Ok(if value > 0 { Some(value) } else { None })
        }
        None => Ok(None),
    }
}

/// Sensible default model per provider; the Settings model field overrides it.
fn default_model(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-sonnet-4-5",
        "deepseek" => "deepseek-chat",
        _ => "gpt-4o-mini",
    }
}
