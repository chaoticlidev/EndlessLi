//! EndlessLi Tauri shell: app lifecycle, IPC wiring, and sidecar ownership.
//!
//! Why this layout exists: `main.rs` stays a two-line entry point while this
//! library owns setup (logging, state, plugins). Command handlers live in
//! [`commands`] and stay thin; domain logic lives in `crates/el-*`.

pub mod commands;
pub mod logging;
pub mod sidecar;
pub mod state;

use tauri::Manager;

/// Bootstraps logging, shared state, and plugins, then runs the app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let log_db_path = logging::init_logging(&data_dir)
                .unwrap_or_else(|_| data_dir.join(logging::LOG_DB_FILENAME));
            app.manage(state::AppState {
                data_dir,
                log_db_path,
            });
            tracing::info!("EndlessLi starting (Phase 0 foundation)");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
