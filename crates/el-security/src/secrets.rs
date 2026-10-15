//! OS keychain access for provider API keys.
//!
//! Keys are stored under one service name with the provider id as the
//! account. They are write-only from the UI's perspective: the only read
//! path is into the LLM client at request time, never back to the frontend.

use el_core::{AppError, AppResult};

/// Keychain service name grouping all EndlessLi secrets.
pub const KEYRING_SERVICE: &str = "endlessli";

/// Provider ids accepted by the key commands.
pub const KNOWN_PROVIDERS: &[&str] = &["openai", "anthropic", "deepseek", "brave"];

/// Rejects unknown provider ids before touching the keychain.
pub fn check_provider(provider: &str) -> AppResult<()> {
    if KNOWN_PROVIDERS.contains(&provider) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "unknown provider {provider}; expected one of {}",
            KNOWN_PROVIDERS.join(", ")
        )))
    }
}

fn entry(provider: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, provider)
        .map_err(|err| AppError::Internal(format!("open keychain entry: {err}")))
}

/// Stores (or replaces) a provider API key. Blank keys are rejected so an
/// accidental empty save cannot lock the user out with a confusing auth error.
pub fn set_api_key(provider: &str, key: &str) -> AppResult<()> {
    check_provider(provider)?;
    if key.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "API key must not be blank".to_string(),
        ));
    }
    entry(provider)?
        .set_password(key)
        .map_err(|err| AppError::Internal(format!("store API key: {err}")))?;
    Ok(())
}

/// Removes a provider API key. Succeeds when no key was stored.
pub fn delete_api_key(provider: &str) -> AppResult<()> {
    check_provider(provider)?;
    match entry(provider)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(AppError::Internal(format!("delete API key: {err}"))),
    }
}

/// Reports whether a key is stored, without revealing it.
pub fn has_api_key(provider: &str) -> AppResult<bool> {
    check_provider(provider)?;
    match entry(provider)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(err) => Err(AppError::Internal(format!("read keychain: {err}"))),
    }
}

/// Loads a key for request signing. This is the only read path, used by the
/// Tauri shell when building LLM clients. It must never be exposed through
/// IPC: keys cross the boundary into memory only, never back to the UI.
pub fn load_api_key(provider: &str) -> AppResult<String> {
    check_provider(provider)?;
    entry(provider)?.get_password().map_err(|err| match err {
        keyring::Error::NoEntry => AppError::InvalidInput(format!(
            "no API key stored for {provider}; add one in Settings"
        )),
        other => AppError::Internal(format!("read keychain: {other}")),
    })
}

#[cfg(all(test, any(target_os = "windows", target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn keychain_roundtrip() {
        let provider = "openai";
        delete_api_key(provider).expect("delete is idempotent");
        assert!(!has_api_key(provider).expect("has"));
        set_api_key(provider, "test-secret").expect("set");
        assert!(has_api_key(provider).expect("has"));
        assert_eq!(load_api_key(provider).expect("load"), "test-secret");
        assert!(set_api_key(provider, "   ").is_err());
        delete_api_key(provider).expect("delete");
        assert!(!has_api_key(provider).expect("has"));
    }

    #[test]
    fn unknown_provider_rejected() {
        assert!(set_api_key("skynet", "x").is_err());
        assert!(has_api_key("skynet").is_err());
    }
}
