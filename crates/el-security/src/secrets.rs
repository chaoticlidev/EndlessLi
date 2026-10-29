//! OS keychain access for provider API keys.
//!
//! Keys are stored under one service name with the provider id as the
//! account. They are write-only from the UI's perspective: the only read
//! path is into the LLM client at request time, never back to the frontend.

use el_core::{AppError, AppResult};

/// Keychain service name grouping all EndlessLi secrets.
pub const KEYRING_SERVICE: &str = "endlessli";

/// Provider ids accepted by the key commands.
///
/// Built-in LLM ids mirror `el_llm::catalog`; search ids mirror the Python
/// sidecar's provider registry. User-defined endpoints and TTS endpoints are
/// accepted through the `custom.` and `tts.` prefixes validated below.
pub const KNOWN_PROVIDERS: &[&str] = &[
    // Built-in language models.
    "openai",
    "anthropic",
    "google",
    "deepseek",
    "groq",
    "cerebras",
    "openrouter",
    "qwen",
    "kimi",
    "minimax",
    "nvidia",
    "fireworks",
    "huggingface",
    "meta",
    "zai",
    "ant-ling",
    "opencode-zen",
    "vercel",
    "cloudflare",
    "github-copilot",
    // Search backends.
    "brave",
    "searxng",
    "tavily",
    "exa",
    "keenable",
    "firecrawl",
    "bing",
    "parallel",
    "anysearch",
    "perplexity",
    "serpbase",
    "you",
];

/// Prefix for user-defined OpenAI-compatible language-model endpoints.
pub const CUSTOM_LLM_PREFIX: &str = "custom.";

/// Prefix for user-defined OpenAI Speech-compatible endpoints.
pub const CUSTOM_TTS_PREFIX: &str = "tts.";

/// Validates the slug after a dynamic prefix.
///
/// Dynamic ids become keychain accounts, so they are restricted to lowercase
/// letters, digits, and dashes. This keeps arbitrary or path-like strings from
/// reaching the OS keychain. The rule is mirrored (not shared) from
/// `el_llm::catalog` because `el-llm` depends on this crate.
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 40
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

/// Whether a provider id is one of the built-in ids.
pub fn is_builtin(provider: &str) -> bool {
    KNOWN_PROVIDERS.contains(&provider)
}

/// Whether a provider id is a validated user-defined endpoint.
pub fn is_dynamic(provider: &str) -> bool {
    [CUSTOM_LLM_PREFIX, CUSTOM_TTS_PREFIX]
        .iter()
        .any(|prefix| provider.strip_prefix(prefix).is_some_and(valid_slug))
}

/// Rejects unknown provider ids before touching the keychain.
pub fn check_provider(provider: &str) -> AppResult<()> {
    if is_builtin(provider) || is_dynamic(provider) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "unknown provider {provider:?}; expected a built-in id or custom.<slug>/tts.<slug>"
        )))
    }
}

fn entry(provider: &str) -> AppResult<keyring::Entry> {
    entry_for(provider)
}

fn entry_for(account: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, account)
        .map_err(|err| AppError::Internal(format!("open keychain entry: {err}")))
}

/// Account name for a project's note-encryption key. Project ids are UUIDs,
/// so the account stays well under platform keychain limits.
fn project_account(project_id: &str) -> String {
    format!("project-key.{project_id}")
}

/// Stores a project's 32-byte note-encryption key (base64-encoded) in the OS
/// keychain. The key never touches SQLite.
pub fn set_project_key(project_id: &str, encoded_key: &str) -> AppResult<()> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "project id must not be blank".to_string(),
        ));
    }
    entry_for(&project_account(project_id))?
        .set_password(encoded_key)
        .map_err(|err| AppError::Internal(format!("store project encryption key: {err}")))?;
    Ok(())
}

/// Loads a project's note-encryption key, or a clear error when it is absent.
pub fn load_project_key(project_id: &str) -> AppResult<String> {
    entry_for(&project_account(project_id))?
        .get_password()
        .map_err(|err| match err {
            keyring::Error::NoEntry => {
                AppError::InvalidInput(format!("no encryption key stored for project {project_id}"))
            }
            other => AppError::Internal(format!("read project encryption key: {other}")),
        })
}

/// Removes a project's note-encryption key. Succeeds when none was stored.
pub fn delete_project_key(project_id: &str) -> AppResult<()> {
    match entry_for(&project_account(project_id))?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(AppError::Internal(format!(
            "delete project encryption key: {err}"
        ))),
    }
}

/// Keychain account holding the optional SQLCipher passphrase.
const DATABASE_KEY_ACCOUNT: &str = "database-key";

/// Stores the SQLCipher passphrase. Only meaningful in a `sqlcipher` build.
pub fn set_database_key(passphrase: &str) -> AppResult<()> {
    if passphrase.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "database passphrase must not be blank".to_string(),
        ));
    }
    entry_for(DATABASE_KEY_ACCOUNT)?
        .set_password(passphrase)
        .map_err(|err| AppError::Internal(format!("store database key: {err}")))?;
    Ok(())
}

/// Loads the SQLCipher passphrase, if one was provisioned.
pub fn load_database_key() -> AppResult<String> {
    entry_for(DATABASE_KEY_ACCOUNT)?
        .get_password()
        .map_err(|err| match err {
            keyring::Error::NoEntry => AppError::InvalidInput("no database key stored".to_string()),
            other => AppError::Internal(format!("read database key: {other}")),
        })
}

/// Removes the SQLCipher passphrase from the keychain.
pub fn delete_database_key() -> AppResult<()> {
    match entry_for(DATABASE_KEY_ACCOUNT)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(AppError::Internal(format!("delete database key: {err}"))),
    }
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

    #[test]
    fn dynamic_ids_are_accepted() {
        assert!(check_provider("custom.local-llama").is_ok());
        assert!(check_provider("tts.my-endpoint").is_ok());
        assert!(check_provider("custom.").is_err());
        assert!(check_provider("custom.Bad").is_err());
        assert!(check_provider("tts.a/b").is_err());
        assert!(check_provider("custom.-lead").is_err());
    }

    #[test]
    fn every_catalog_id_is_known() {
        for id in [
            "google",
            "groq",
            "cerebras",
            "openrouter",
            "qwen",
            "kimi",
            "minimax",
            "nvidia",
            "fireworks",
            "huggingface",
            "meta",
            "zai",
            "ant-ling",
            "opencode-zen",
            "vercel",
            "cloudflare",
            "github-copilot",
        ] {
            assert!(check_provider(id).is_ok(), "{id} must be accepted");
        }
    }
}
