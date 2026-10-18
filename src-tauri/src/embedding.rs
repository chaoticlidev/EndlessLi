//! Note-embedding pipeline: the sidecar-backed embedder and the debounced
//! background worker that keeps note vectors current.
//!
//! Reasoning lives in `el-linking`; this module is the adapter layer.
//! [`SidecarEmbedder`] implements [`el_linking::Embedder`] over the sidecar's
//! `embed.*` RPCs. [`run_worker`] drains the shared [`EmbedQueue`] with a
//! short debounce so a burst of edits produces one embedding pass per note,
//! then emits fresh link candidates for each embedded note.

use std::time::Duration;

use el_core::{AppError, AppResult, EmbeddingModelInfo};
use el_linking::Embedder;
use serde_json::json;
use tauri::AppHandle;
use tauri_specta::Event as _;

use crate::commands::links::LinkingSuggestionsEvent;
use crate::sidecar::SidecarManager;
use crate::state::AppState;

/// Wait after the first enqueue before embedding, coalescing rapid edits.
pub const DEBOUNCE_MS: u64 = 1_500;

/// Embedder backed by the Python sidecar's `embed.model_info` / `embed.texts`.
#[derive(Debug, Clone)]
pub struct SidecarEmbedder {
    sidecar: SidecarManager,
}

impl SidecarEmbedder {
    /// Wraps a sidecar manager.
    pub fn new(sidecar: SidecarManager) -> Self {
        Self { sidecar }
    }
}

impl Embedder for SidecarEmbedder {
    async fn model_info(&self, model: &str) -> AppResult<EmbeddingModelInfo> {
        let value = self
            .sidecar
            .call("embed.model_info", json!({ "model": model }))
            .await?;
        let dim = value.get("dim").and_then(|dim| dim.as_u64()).unwrap_or(0);
        Ok(EmbeddingModelInfo {
            model: value
                .get("model")
                .and_then(|model| model.as_str())
                .unwrap_or(model)
                .to_string(),
            dim: u32::try_from(dim).unwrap_or(0),
            available: value
                .get("available")
                .and_then(|available| available.as_bool())
                .unwrap_or(false),
            is_default: value
                .get("is_default")
                .and_then(|is_default| is_default.as_bool())
                .unwrap_or(false),
        })
    }

    async fn embed(&self, texts: &[String], model: &str) -> AppResult<Vec<Vec<f32>>> {
        let value = self
            .sidecar
            .call("embed.texts", json!({ "texts": texts, "model": model }))
            .await?;
        let embeddings = value
            .get("embeddings")
            .and_then(|embeddings| embeddings.as_array())
            .ok_or_else(|| {
                AppError::Internal("embed.texts response is missing embeddings".to_string())
            })?;
        let mut out = Vec::with_capacity(embeddings.len());
        for vector in embeddings {
            let values = vector.as_array().ok_or_else(|| {
                AppError::Internal("embed.texts returned a non-array vector".to_string())
            })?;
            out.push(
                values
                    .iter()
                    .map(|value| value.as_f64().unwrap_or(0.0) as f32)
                    .collect(),
            );
        }
        Ok(out)
    }
}

/// Reads the active embedding model from settings, defaulting to MiniLM.
pub fn active_model(state: &AppState) -> AppResult<String> {
    let conn = state.db.connect()?;
    let raw = el_db::repo::cache::get_setting(&conn, el_linking::SETTING_EMBEDDING_MODEL)?;
    Ok(el_linking::resolve_model(raw.as_deref()))
}

/// Reads the auto-linking threshold from settings, defaulting to 0.75.
pub fn active_threshold(state: &AppState) -> AppResult<f64> {
    let conn = state.db.connect()?;
    let raw = el_db::repo::cache::get_setting(&conn, el_linking::SETTING_LINKING_THRESHOLD)?;
    Ok(el_linking::threshold_from_setting(raw.as_deref()))
}

/// Reconciles the vector table with the active model (idempotent).
pub async fn prepare(
    state: &AppState,
    embedder: &SidecarEmbedder,
    model: &str,
) -> AppResult<EmbeddingModelInfo> {
    el_linking::ensure_embeddings_ready(&state.db, embedder, model).await
}

/// The embedding worker. Spawned once for the app's lifetime; loops on the
/// queue, debouncing bursts and degrading quietly when the sidecar is down.
pub async fn run_worker(app: AppHandle, state: AppState) {
    let embedder = SidecarEmbedder::new(state.sidecar.clone());
    let queue = state.embed.clone();
    loop {
        queue.notified().await;
        tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;

        let model = match active_model(&state) {
            Ok(model) => model,
            Err(err) => {
                tracing::warn!(error = %err, "embed worker could not read the model setting");
                continue;
            }
        };
        if let Err(err) = prepare(&state, &embedder, &model).await {
            tracing::warn!(error = %err, "embed worker could not prepare the vector table");
            continue;
        }

        loop {
            let ids = queue.drain();
            if ids.is_empty() {
                break;
            }
            for id in ids {
                match el_linking::embed_note(&state.db, &embedder, &model, &id).await {
                    Ok(true) => emit_suggestions(&app, &state, &id, &model),
                    Ok(false) => {}
                    Err(err) => {
                        tracing::warn!(note = %id, error = %err, "embed worker failed to embed note")
                    }
                }
            }
        }
    }
}

/// Computes and emits the current candidates for a note after it is embedded.
fn emit_suggestions(app: &AppHandle, state: &AppState, note_id: &str, model: &str) {
    let threshold = active_threshold(state).unwrap_or(el_linking::DEFAULT_SIMILARITY_THRESHOLD);
    match el_linking::suggest_links(
        &state.db,
        note_id,
        model,
        threshold,
        el_linking::MAX_CANDIDATES,
    ) {
        Ok(candidates) => {
            let _ = LinkingSuggestionsEvent {
                note_id: note_id.to_string(),
                candidates,
            }
            .emit(app);
        }
        Err(err) => {
            tracing::warn!(note = %note_id, error = %err, "embed worker failed to suggest links")
        }
    }
}
