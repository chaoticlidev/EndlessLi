//! Text-to-speech configuration and synthesis.
//!
//! Three engines are supported:
//!
//! * `kokoro`  local Kokoro-ONNX model (runs in the Python sidecar)
//! * `piper`   local Piper voice model (spawns the Piper executable)
//! * `openai`  any OpenAI Speech-compatible endpoint (`/audio/speech`)
//!
//! Local engines keep audio on the machine. Remote endpoint keys live in the
//! OS keychain under `tts.<slug>`, exactly like LLM keys, and are never
//! returned to the frontend. Audio comes back as a data URL so the webview can
//! preview it without a second IPC round trip.

use el_core::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// Setting key holding the whole audio configuration as JSON.
pub const SETTING_AUDIO_CONFIG: &str = "audio.config";

/// Text spoken by the "Play sample" button.
pub const SAMPLE_TEXT: &str =
    "This is a preview of the selected voice. EndlessLi can read your notes aloud.";

/// Supported engines.
pub const ENGINES: &[&str] = &["kokoro", "piper", "openai"];

/// Voice ids shipped by OpenAI's speech API; used as suggestions when a custom
/// endpoint does not publish its own list.
pub const OPENAI_VOICES: &[&str] = &[
    "alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse",
];

/// Suggested Kokoro voices. The real list comes from the voices JSON file when
/// one is configured; this keeps the dropdown useful before that.
pub const KOKORO_VOICES: &[&str] = &[
    "af_heart",
    "af_bella",
    "af_nicole",
    "af_sarah",
    "af_sky",
    "am_adam",
    "am_michael",
    "bf_emma",
    "bf_isabella",
    "bm_george",
    "bm_lewis",
];

const MAX_ENDPOINTS: usize = 20;
const MAX_VOICES: usize = 200;

/// One user-defined OpenAI Speech-compatible endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TtsCustomProvider {
    /// `tts.<slug>`; also the keychain account name.
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub voices: Vec<String>,
}

/// Audio settings, persisted as one JSON blob under `audio.config`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TtsConfig {
    /// `kokoro`, `piper`, or `openai`.
    pub engine: String,
    /// Selected voice id for the active engine.
    pub voice: String,
    pub kokoro_model_path: String,
    pub kokoro_voices_path: String,
    pub piper_path: String,
    pub piper_voices_dir: String,
    pub custom_providers: Vec<TtsCustomProvider>,
    /// Selected custom endpoint id (empty when none is chosen).
    pub custom_provider_id: String,
    pub custom_model: String,
    pub custom_voice: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            engine: "kokoro".to_string(),
            voice: String::new(),
            kokoro_model_path: String::new(),
            kokoro_voices_path: String::new(),
            piper_path: String::new(),
            piper_voices_dir: String::new(),
            custom_providers: Vec::new(),
            custom_provider_id: String::new(),
            custom_model: String::new(),
            custom_voice: String::new(),
        }
    }
}

/// One selectable voice.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TtsVoice {
    pub id: String,
    pub label: String,
}

/// A synthesized sample plus enough metadata for the UI to show what played.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TtsSample {
    /// `data:<mime>;base64,<payload>` for direct playback.
    pub data_url: String,
    pub mime: String,
    pub engine: String,
    pub voice: String,
    #[specta(type = specta_typescript::Number)]
    pub bytes: i64,
}

/// Optional overrides for previewing settings that have not been saved yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct TtsSpeakRequest {
    pub engine: Option<String>,
    pub voice: Option<String>,
    pub text: Option<String>,
    pub custom_provider_id: Option<String>,
    pub custom_model: Option<String>,
    pub custom_base_url: Option<String>,
    pub kokoro_model_path: Option<String>,
    pub kokoro_voices_path: Option<String>,
    pub piper_path: Option<String>,
    pub piper_voices_dir: Option<String>,
}

/// Reads the audio configuration.
#[tauri::command]
#[specta::specta]
pub fn tts_config_get(state: State<'_, AppState>) -> AppResult<TtsConfig> {
    load_tts_config(&state)
}

/// Validates and persists the audio configuration.
#[tauri::command]
#[specta::specta]
pub fn tts_config_save(state: State<'_, AppState>, config: TtsConfig) -> AppResult<TtsConfig> {
    save_tts_config(&state, config)
}

/// Lists voices for an engine, using unsaved path overrides when supplied.
#[tauri::command]
#[specta::specta]
pub async fn tts_voices(
    state: State<'_, AppState>,
    engine: String,
    custom_provider_id: Option<String>,
    piper_voices_dir: Option<String>,
    kokoro_voices_path: Option<String>,
) -> AppResult<Vec<TtsVoice>> {
    let config = load_tts_config(&state)?;
    voices_for(
        &state,
        &config,
        &engine,
        custom_provider_id.as_deref(),
        piper_voices_dir.as_deref(),
        kokoro_voices_path.as_deref(),
    )
    .await
}

/// Synthesizes speech and returns a data URL for playback.
#[tauri::command]
#[specta::specta]
pub async fn tts_synthesize(
    state: State<'_, AppState>,
    request: TtsSpeakRequest,
) -> AppResult<TtsSample> {
    let config = load_tts_config(&state)?;
    synthesize(&state, &config, request).await
}

/// Custom TTS endpoint ids, for the key-status list.
pub fn custom_provider_ids(state: &AppState) -> AppResult<Vec<String>> {
    let config = load_tts_config(state)?;
    Ok(config
        .custom_providers
        .into_iter()
        .map(|custom| custom.id)
        .collect())
}

/// Reads `audio.config`, falling back to defaults.
pub fn load_tts_config(state: &AppState) -> AppResult<TtsConfig> {
    let conn = state.db.connect()?;
    match el_db::repo::cache::get_setting(&conn, SETTING_AUDIO_CONFIG)? {
        Some(raw) => serde_json::from_str(&raw).map_err(|err| {
            AppError::InvalidInput(format!("{SETTING_AUDIO_CONFIG} is not valid JSON: {err}"))
        }),
        None => Ok(TtsConfig::default()),
    }
}

fn save_tts_config(state: &AppState, mut config: TtsConfig) -> AppResult<TtsConfig> {
    normalize(&mut config);
    validate(&config)?;
    let previous = load_tts_config(state)?;
    let encoded = serde_json::to_string(&config)
        .map_err(|err| AppError::Internal(format!("serialize audio config: {err}")))?;
    {
        let conn = state.db.connect()?;
        el_db::repo::cache::set_setting(&conn, SETTING_AUDIO_CONFIG, &encoded)?;
    }
    let kept: std::collections::BTreeSet<&str> = config
        .custom_providers
        .iter()
        .map(|custom| custom.id.as_str())
        .collect();
    for custom in &previous.custom_providers {
        if !kept.contains(custom.id.as_str()) {
            if let Err(err) = el_security::delete_api_key(&custom.id) {
                tracing::warn!(provider = %custom.id, %err, "could not delete removed TTS key");
            }
        }
    }
    Ok(config)
}

fn normalize(config: &mut TtsConfig) {
    config.engine = config.engine.trim().to_lowercase();
    config.voice = config.voice.trim().to_string();
    config.kokoro_model_path = config.kokoro_model_path.trim().to_string();
    config.kokoro_voices_path = config.kokoro_voices_path.trim().to_string();
    config.piper_path = config.piper_path.trim().to_string();
    config.piper_voices_dir = config.piper_voices_dir.trim().to_string();
    config.custom_provider_id = config.custom_provider_id.trim().to_string();
    config.custom_model = config.custom_model.trim().to_string();
    config.custom_voice = config.custom_voice.trim().to_string();
    for custom in &mut config.custom_providers {
        custom.id = custom.id.trim().to_lowercase();
        custom.display_name = custom.display_name.trim().to_string();
        custom.base_url = custom.base_url.trim().trim_end_matches('/').to_string();
        clean_list(&mut custom.models);
        clean_list(&mut custom.voices);
    }
    config.custom_providers.truncate(MAX_ENDPOINTS);
}

fn clean_list(values: &mut Vec<String>) {
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.retain(|value| !value.is_empty());
    values.dedup();
    values.truncate(MAX_VOICES);
}

fn validate(config: &TtsConfig) -> AppResult<()> {
    if !ENGINES.contains(&config.engine.as_str()) {
        return Err(AppError::InvalidInput(format!(
            "unknown TTS engine {:?}; expected one of {}",
            config.engine,
            ENGINES.join(", ")
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for custom in &config.custom_providers {
        if !el_security::secrets::is_dynamic(&custom.id) || !custom.id.starts_with("tts.") {
            return Err(AppError::InvalidInput(format!(
                "invalid TTS endpoint id {:?}; expected tts.<lowercase-letters-digits-dashes>",
                custom.id
            )));
        }
        if !seen.insert(custom.id.as_str()) {
            return Err(AppError::InvalidInput(
                "TTS endpoint ids must be unique".to_string(),
            ));
        }
        if custom.display_name.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "TTS endpoint {} needs a display name",
                custom.id
            )));
        }
        let url = custom.base_url.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(AppError::InvalidInput(format!(
                "TTS endpoint URL must start with http:// or https://, got {url:?}"
            )));
        }
    }
    if config.engine == "openai"
        && !config.custom_provider_id.is_empty()
        && !config
            .custom_providers
            .iter()
            .any(|custom| custom.id == config.custom_provider_id)
    {
        return Err(AppError::InvalidInput(format!(
            "selected TTS endpoint {} is not configured",
            config.custom_provider_id
        )));
    }
    Ok(())
}

async fn voices_for(
    state: &AppState,
    config: &TtsConfig,
    engine: &str,
    custom_provider_id: Option<&str>,
    piper_voices_dir: Option<&str>,
    kokoro_voices_path: Option<&str>,
) -> AppResult<Vec<TtsVoice>> {
    match engine {
        "piper" => {
            let dir = piper_voices_dir
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&config.piper_voices_dir);
            if dir.is_empty() {
                return Ok(Vec::new());
            }
            let result = state
                .sidecar
                .call(
                    "tts.voices",
                    serde_json::json!({ "engine": "piper", "voices_dir": dir }),
                )
                .await?;
            Ok(parse_voices(&result))
        }
        "kokoro" => {
            let path = kokoro_voices_path
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&config.kokoro_voices_path);
            let mut voices = if path.is_empty() {
                Vec::new()
            } else {
                match state
                    .sidecar
                    .call(
                        "tts.voices",
                        serde_json::json!({ "engine": "kokoro", "voices_path": path }),
                    )
                    .await
                {
                    Ok(result) => parse_voices(&result),
                    Err(err) => {
                        // A missing or unreadable voices file should not hide
                        // the suggested voices; synthesis reports the real error.
                        tracing::warn!(%err, "could not read Kokoro voices");
                        Vec::new()
                    }
                }
            };
            if voices.is_empty() {
                voices = KOKORO_VOICES
                    .iter()
                    .map(|voice| TtsVoice {
                        id: (*voice).to_string(),
                        label: (*voice).to_string(),
                    })
                    .collect();
            }
            Ok(voices)
        }
        "openai" => {
            let provider = custom_provider_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&config.custom_provider_id);
            let configured = config
                .custom_providers
                .iter()
                .find(|custom| custom.id == provider)
                .map(|custom| custom.voices.clone())
                .unwrap_or_default();
            let ids: Vec<String> = if configured.is_empty() {
                OPENAI_VOICES
                    .iter()
                    .map(|voice| (*voice).to_string())
                    .collect()
            } else {
                configured
            };
            Ok(ids
                .into_iter()
                .map(|voice| TtsVoice {
                    label: voice.clone(),
                    id: voice,
                })
                .collect())
        }
        other => Err(AppError::InvalidInput(format!(
            "unknown TTS engine {other:?}; expected one of {}",
            ENGINES.join(", ")
        ))),
    }
}

fn parse_voices(value: &serde_json::Value) -> Vec<TtsVoice> {
    let Some(array) = value.get("voices").and_then(|voices| voices.as_array()) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|entry| {
            let id = entry
                .get("id")
                .and_then(|id| id.as_str())
                .or_else(|| entry.as_str())?;
            let label = entry
                .get("label")
                .and_then(|label| label.as_str())
                .unwrap_or(id);
            Some(TtsVoice {
                id: id.to_string(),
                label: label.to_string(),
            })
        })
        .collect()
}

async fn synthesize(
    state: &AppState,
    config: &TtsConfig,
    request: TtsSpeakRequest,
) -> AppResult<TtsSample> {
    let engine = request
        .engine
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&config.engine)
        .to_lowercase();
    if !ENGINES.contains(&engine.as_str()) {
        return Err(AppError::InvalidInput(format!(
            "unknown TTS engine {engine:?}"
        )));
    }
    let text = request
        .text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(SAMPLE_TEXT)
        .to_string();
    if text.chars().count() > 8_000 {
        return Err(AppError::InvalidInput(
            "text exceeds the 8000-character TTS limit".to_string(),
        ));
    }
    let mut params = serde_json::json!({
        "engine": engine,
        "text": text,
        "piper_path": request.piper_path.as_deref().map(str::trim).filter(|v| !v.is_empty()).unwrap_or(&config.piper_path),
        "piper_voices_dir": request.piper_voices_dir.as_deref().map(str::trim).filter(|v| !v.is_empty()).unwrap_or(&config.piper_voices_dir),
        "kokoro_model_path": request.kokoro_model_path.as_deref().map(str::trim).filter(|v| !v.is_empty()).unwrap_or(&config.kokoro_model_path),
        "kokoro_voices_path": request.kokoro_voices_path.as_deref().map(str::trim).filter(|v| !v.is_empty()).unwrap_or(&config.kokoro_voices_path),
    });
    let voice = match engine.as_str() {
        "openai" => {
            let provider_id = request
                .custom_provider_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&config.custom_provider_id);
            let provider = config
                .custom_providers
                .iter()
                .find(|custom| custom.id == provider_id)
                .ok_or_else(|| {
                    AppError::InvalidInput(
                        "choose a custom speech endpoint and save it before playing a sample"
                            .to_string(),
                    )
                })?;
            let base_url = request
                .custom_base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&provider.base_url);
            let model = request
                .custom_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    (!config.custom_model.is_empty()).then_some(config.custom_model.as_str())
                })
                .or_else(|| provider.models.first().map(String::as_str))
                .ok_or_else(|| {
                    AppError::InvalidInput("enter the endpoint's speech model name".to_string())
                })?;
            let voice = request
                .voice
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    (!config.custom_voice.is_empty()).then_some(config.custom_voice.as_str())
                })
                .or_else(|| provider.voices.first().map(String::as_str))
                .unwrap_or("alloy");
            let api_key = el_security::load_api_key(&provider.id).unwrap_or_default();
            params["base_url"] = serde_json::json!(base_url);
            params["api_key"] = serde_json::json!(api_key);
            params["model"] = serde_json::json!(model);
            voice.to_string()
        }
        _ => request
            .voice
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&config.voice)
            .to_string(),
    };
    if voice.is_empty() {
        return Err(AppError::InvalidInput(
            "choose a voice before playing a sample".to_string(),
        ));
    }
    params["voice"] = serde_json::json!(voice);

    let result = state.sidecar.call("tts.synthesize", params).await?;
    let base64 = result
        .get("audio_base64")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Internal("sidecar returned no audio".to_string()))?;
    // 25 MiB of audio is ~33 MiB of base64; anything larger is not a sample.
    if base64.len() > 40 * 1024 * 1024 {
        return Err(AppError::Internal(
            "synthesized audio is too large for the UI".to_string(),
        ));
    }
    let mime = result
        .get("mime")
        .and_then(|value| value.as_str())
        .unwrap_or("audio/wav")
        .to_string();
    let bytes = (base64.len() as i64 * 3) / 4;
    Ok(TtsSample {
        data_url: format!("data:{mime};base64,{base64}"),
        mime,
        engine,
        voice,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let config = TtsConfig::default();
        assert!(validate(&config).is_ok());
        assert_eq!(config.engine, "kokoro");
    }

    #[test]
    fn unknown_engine_rejected() {
        let config = TtsConfig {
            engine: "espeak".to_string(),
            ..Default::default()
        };
        // Actually test the validation
        assert!(validate(&config).is_err());
    }

    #[test]
    fn custom_endpoint_ids_are_validated() {
        let mut config = TtsConfig::default();
        config.custom_providers.push(TtsCustomProvider {
            id: "custom.bad".to_string(),
            display_name: "Bad".to_string(),
            base_url: "https://speech.example.com/v1".to_string(),
            models: vec!["tts-1".to_string()],
            voices: vec!["alloy".to_string()],
        });
        assert!(validate(&config).is_err());

        config.custom_providers[0].id = "tts.local".to_string();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn endpoint_urls_must_be_http() {
        let mut config = TtsConfig::default();
        config.custom_providers.push(TtsCustomProvider {
            id: "tts.local".to_string(),
            display_name: "Local".to_string(),
            base_url: "file:///tmp/speech".to_string(),
            models: vec!["tts-1".to_string()],
            voices: Vec::new(),
        });
        assert!(validate(&config).is_err());
    }

    #[test]
    fn voices_parse_from_sidecar() {
        let value = serde_json::json!({
            "voices": ["af_heart", { "id": "en_US-lessac-medium", "label": "Lessac" }]
        });
        let voices = parse_voices(&value);
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].id, "af_heart");
        assert_eq!(voices[1].label, "Lessac");
    }
}
