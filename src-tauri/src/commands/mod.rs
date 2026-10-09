//! Thin Tauri IPC handlers (one module per area from Phase 1 onward).
//!
//! Why this exists: handlers validate input, call into `el-*` crates, and map
//! errors to [`el_core::AppError`]. No business logic lives here, which keeps
//! handlers unit-testable and the IPC surface (plan Section 6) auditable.

use el_core::{AppError, AppResult};
use tauri::State;

use crate::state::AppState;

/// Placeholder health check proving the IPC bridge works end to end.
///
/// Removed or replaced by real `project_*` / `note_*` commands in Phase 2.
#[tauri::command]
pub fn greet(name: &str, state: State<'_, AppState>) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("name must not be empty".to_string()));
    }
    if trimmed.len() > 200 {
        return Err(AppError::InvalidInput(
            "name must be at most 200 characters".to_string(),
        ));
    }
    tracing::info!(data_dir = %state.data_dir.display(), "greet invoked");
    Ok(format!("Hello, {trimmed}! You've been greeted from Rust!"))
}
