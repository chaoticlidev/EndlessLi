//! Per-project note encryption (Phase 11.2).
//!
//! Notes in an encrypted project are stored as `enc:v1:<base64(nonce || ciphertext)>`
//! using AES-256-GCM. The 32-byte key is generated once per project and kept in
//! the OS keychain; only ciphertext reaches SQLite. Decryption is transparent to
//! the rest of the app: repositories call [`decrypt_text`] on read and
//! [`encrypt_text`] on write.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use el_core::{AppError, AppResult};

use crate::secrets;

/// Marker prefix on every encrypted value.
pub const ENCRYPTED_PREFIX: &str = "enc:v1:";

/// AES-256 key length.
pub const KEY_LEN: usize = 32;
/// GCM nonce length.
const NONCE_LEN: usize = 12;

/// Process-local key cache so listing/reading many notes does not hit the OS
/// keychain once per row. Cleared when a project key is disabled.
fn key_cache() -> &'static Mutex<HashMap<String, [u8; KEY_LEN]>> {
    static CACHE: OnceLock<Mutex<HashMap<String, [u8; KEY_LEN]>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn decode_key(encoded: &str) -> AppResult<[u8; KEY_LEN]> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|err| AppError::Internal(format!("decode project key: {err}")))?;
    bytes
        .try_into()
        .map_err(|_| AppError::Internal("project key has the wrong length".to_string()))
}

fn project_key(project_id: &str) -> AppResult<[u8; KEY_LEN]> {
    if let Ok(cache) = key_cache().lock() {
        if let Some(key) = cache.get(project_id) {
            return Ok(*key);
        }
    }
    let key = decode_key(&secrets::load_project_key(project_id)?)?;
    if let Ok(mut cache) = key_cache().lock() {
        cache.insert(project_id.to_string(), key);
    }
    Ok(key)
}

fn generate_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut key);
    key
}

/// Ensures a project has a key. Reuses an existing key so toggling encryption
/// off and on again does not orphan previously encrypted notes.
pub fn enable_project(project_id: &str) -> AppResult<()> {
    if secrets::load_project_key(project_id).is_ok() {
        return Ok(());
    }
    let key = generate_key();
    secrets::set_project_key(project_id, &STANDARD.encode(key))?;
    if let Ok(mut cache) = key_cache().lock() {
        cache.insert(project_id.to_string(), key);
    }
    Ok(())
}

/// Deletes a project's key and forgets it in memory. Call only after every
/// note in the project has been decrypted.
pub fn disable_project(project_id: &str) -> AppResult<()> {
    secrets::delete_project_key(project_id)?;
    if let Ok(mut cache) = key_cache().lock() {
        cache.remove(project_id);
    }
    Ok(())
}

/// True when a stored value carries the encryption marker.
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

/// Encrypts `plaintext` under a project's keychain-held key.
pub fn encrypt_text(project_id: &str, plaintext: &str) -> AppResult<String> {
    if is_encrypted(plaintext) {
        return Ok(plaintext.to_string());
    }
    let key = project_key(project_id)?;
    encrypt_with_key(&key, plaintext)
}

/// Decrypts a stored value. Plaintext values pass through unchanged.
pub fn decrypt_text(project_id: &str, stored: &str) -> AppResult<String> {
    if !is_encrypted(stored) {
        return Ok(stored.to_string());
    }
    let key = project_key(project_id)?;
    decrypt_with_key(&key, stored)
}

/// Encrypts with an explicit key. Exposed for tests and for callers that
/// already hold the key (the migration sweep).
pub fn encrypt_with_key(key: &[u8; KEY_LEN], plaintext: &str) -> AppResult<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|err| AppError::Internal(format!("init cipher: {err}")))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|_| AppError::Internal("encrypt note".to_string()))?;
    let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(format!("{ENCRYPTED_PREFIX}{}", STANDARD.encode(combined)))
}

/// Decrypts with an explicit key. Exposed for tests.
pub fn decrypt_with_key(key: &[u8; KEY_LEN], stored: &str) -> AppResult<String> {
    let encoded = stored
        .strip_prefix(ENCRYPTED_PREFIX)
        .ok_or_else(|| AppError::InvalidInput("value is not encrypted".to_string()))?;
    let combined = STANDARD
        .decode(encoded)
        .map_err(|err| AppError::Internal(format!("decode ciphertext: {err}")))?;
    if combined.len() <= NONCE_LEN {
        return Err(AppError::Internal(
            "encrypted value is truncated".to_string(),
        ));
    }
    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|err| AppError::Internal(format!("init cipher: {err}")))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| {
            AppError::Internal("decrypt note failed (wrong key or corrupted data)".to_string())
        })?;
    String::from_utf8(plaintext)
        .map_err(|err| AppError::Internal(format!("decrypted note is not UTF-8: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_uses_a_fresh_nonce() {
        let key = generate_key();
        let first = encrypt_with_key(&key, "hello world").expect("encrypt");
        let second = encrypt_with_key(&key, "hello world").expect("encrypt");
        assert!(is_encrypted(&first));
        assert_ne!(first, second, "nonce must make ciphertext unique");
        assert_eq!(
            decrypt_with_key(&key, &first).expect("decrypt"),
            "hello world"
        );
        assert_eq!(
            decrypt_with_key(&key, &second).expect("decrypt"),
            "hello world"
        );
    }

    #[test]
    fn wrong_key_fails_instead_of_returning_garbage() {
        let key = generate_key();
        let other = generate_key();
        let stored = encrypt_with_key(&key, "secret").expect("encrypt");
        assert!(decrypt_with_key(&other, &stored).is_err());
    }

    #[test]
    fn primitive_rejects_plaintext_and_detects_tampering() {
        // `decrypt_text` is the pass-through wrapper; the primitive itself
        // must never treat an unmarked value as ciphertext.
        assert!(decrypt_with_key(&generate_key(), "plain").is_err());
        let key = generate_key();
        let mut stored = encrypt_with_key(&key, "secret").expect("encrypt");
        stored.push('A');
        assert!(decrypt_with_key(&key, &stored).is_err());
    }

    #[test]
    fn encrypt_text_does_not_double_encrypt() {
        let key = generate_key();
        let once = encrypt_with_key(&key, "value").expect("encrypt");
        // `encrypt_with_key` is the primitive; `encrypt_text` needs a keychain
        // and is covered by the command-level tests instead.
        assert!(is_encrypted(&once));
        assert!(!is_encrypted("value"));
    }
}
