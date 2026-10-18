//! Embedding commands: model selection, coverage status, and manual re-embed.

use el_core::{AppError, AppResult, EmbeddingStatus};
use el_linking::Embedder;
use tauri::State;

use crate::embedding::{self, SidecarEmbedder};
use crate::state::AppState;

/// Cap for a bulk re-embed enqueue; far above any realistic note count, so it
/// means "all notes that need it".
const REEMBED_LIMIT: usize = 100_000;

/// Reports the active model, its dimension and availability, and how many
/// notes are embedded with it.
#[tauri::command]
#[specta::specta]
pub async fn embedding_status(state: State<'_, AppState>) -> AppResult<EmbeddingStatus> {
    let model = embedding::active_model(&state)?;
    let embedder = SidecarEmbedder::new(state.sidecar.clone());
    let info = match embedder.model_info(&model).await {
        Ok(info) => info,
        Err(err) => {
            tracing::warn!(error = %err, "embedding status: sidecar unavailable");
            el_core::EmbeddingModelInfo {
                model,
                dim: 0,
                available: false,
                is_default: false,
            }
        }
    };
    el_linking::embedding_status(&state.db, &info.model, info.dim, info.available)
}

/// Switches the embedding model: persists it, reconciles the vector table,
/// then queues a full re-embed in the background. Returns the new status.
#[tauri::command]
#[specta::specta]
pub async fn embedding_set_model(
    state: State<'_, AppState>,
    model: String,
) -> AppResult<EmbeddingStatus> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err(AppError::InvalidInput(
            "model must not be blank".to_string(),
        ));
    }
    {
        let conn = state.db.connect()?;
        el_db::repo::cache::set_setting(&conn, el_linking::SETTING_EMBEDDING_MODEL, &model)?;
    }
    let embedder = SidecarEmbedder::new(state.sidecar.clone());
    let info = embedding::prepare(&state, &embedder, &model).await?;
    let pending = {
        let conn = state.db.connect()?;
        el_db::repo::notes::list_needing_embedding(&conn, &info.model, REEMBED_LIMIT)?
    };
    state.embed.enqueue_all(pending);
    el_linking::embedding_status(&state.db, &info.model, info.dim, info.available)
}

/// Queues every note that lacks a current vector for the active model.
#[tauri::command]
#[specta::specta]
pub fn embedding_reindex(state: State<'_, AppState>) -> AppResult<()> {
    let model = embedding::active_model(&state)?;
    let pending = {
        let conn = state.db.connect()?;
        el_db::repo::notes::list_needing_embedding(&conn, &model, REEMBED_LIMIT)?
    };
    tracing::info!(queued = pending.len(), model = %model, "re-embed queued");
    state.embed.enqueue_all(pending);
    Ok(())
}
