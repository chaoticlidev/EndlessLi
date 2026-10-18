//! Analytics commands: reporting whether the DuckDB store is available and
//! how many chunks it holds. Compiled in both feature configurations; the
//! `available` flag is false when the `analytics` feature is off or the
//! database could not be opened.

use el_core::AppResult;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// Analytical store status for diagnostics and the Settings panel.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct AnalyticsStatus {
    pub available: bool,
    pub path: String,
    #[specta(type = specta_typescript::Number)]
    pub chunks: u64,
}

/// Reports the analytics database path, availability, and chunk count.
#[tauri::command]
#[specta::specta]
pub fn analytics_status(state: State<'_, AppState>) -> AppResult<AnalyticsStatus> {
    let path = state
        .data_dir
        .join(el_analytics::ANALYTICS_DB_FILENAME)
        .display()
        .to_string();
    #[cfg(feature = "analytics")]
    if let Some(analytics) = &state.analytics {
        return Ok(AnalyticsStatus {
            available: true,
            path,
            chunks: analytics.count_chunks()?,
        });
    }
    Ok(AnalyticsStatus {
        available: false,
        path,
        chunks: 0,
    })
}
