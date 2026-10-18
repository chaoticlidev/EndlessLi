//! EndlessLi Tauri shell: app lifecycle, IPC wiring, and sidecar ownership.
//!
//! `main.rs` stays a two-line entry point while this library owns setup
//! (logging, database, sidecar, state, plugins). Command handlers live in
//! [`commands`] and stay thin; domain logic lives in `crates/el-*`.
//! TypeScript bindings for every command are generated into
//! `src/ipc/bindings.ts` on debug builds and in the export test.

pub mod commands;
pub mod logging;
pub mod sidecar;
pub mod state;

#[cfg(feature = "analytics")]
pub mod analytics;
pub mod embedding;

use specta_typescript::Typescript;
use tauri::{Emitter, Manager};
use tauri_specta::{collect_commands, collect_events, Builder, Event as SpectaEvent};

use commands::{
    analytics as analytics_commands, cache, compression as compression_commands, embedding as embedding_commands, folders, links, memory as memory_commands, notes,
    projects, research, secrets, settings, tags,
};

/// The full IPC surface, exposed for the bindings export test.
/// Both `run` and codegen build from this one list so bindings can never
/// drift from the handler registration.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            projects::project_list,
            projects::project_create,
            projects::project_update,
            projects::project_archive,
            projects::project_delete,
            folders::folder_tree,
            folders::folder_create,
            folders::folder_rename,
            folders::folder_move,
            folders::folder_delete,
            notes::note_list,
            notes::note_get,
            notes::note_detail,
            notes::note_create,
            notes::note_update,
            notes::note_move,
            notes::note_delete,
            notes::note_branch,
            notes::note_search,
            notes::note_find_by_title,
            tags::tag_list,
            tags::tag_create,
            tags::tag_assign,
            tags::tag_unassign,
            tags::tag_delete,
            links::link_create_manual,
            links::link_remove,
            links::links_for_note,
            links::link_reject,
            links::link_suggestions,
            links::link_approve,
            memory_commands::memory_list,
            memory_commands::memory_create,
            memory_commands::memory_update,
            memory_commands::memory_forget,
            memory_commands::memory_delete,
            memory_commands::memory_suggestions,
            memory_commands::memory_accept_suggestion,
            memory_commands::memory_reject_suggestion,
            memory_commands::memory_set_enabled,
            memory_commands::memory_is_enabled,
            memory_commands::memory_build_context,
            memory_commands::memory_sweep_expired,
            embedding_commands::embedding_status,
            embedding_commands::embedding_set_model,
            embedding_commands::embedding_reindex,
            analytics_commands::analytics_status,
            settings::settings_get,
            settings::settings_update,
            secrets::api_key_set,
            secrets::api_key_delete,
            secrets::api_key_status,
            secrets::api_key_test,
            secrets::token_usage_summary,
            cache::cache_stats,
            cache::cache_clear,
            research::research_start,
            research::research_cancel,
            research::research_get,
            research::research_history,
            research::research_save_results,
            research::sidecar_status,
            compression_commands::compression_start,
            compression_commands::compression_save_outputs,
        ])
        .events(collect_events![
            research::ResearchStepEvent,
            research::ResearchResultEvent,
            research::ResearchStatusEvent,
            links::LinkingSuggestionsEvent,
        ])
}

/// Bootstraps logging, the database, the sidecar, shared state, event
/// forwarding, and plugins, then runs.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    builder
        .export(Typescript::default(), "../src/ipc/bindings.ts")
        .expect("failed to export TypeScript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let log_db_path = logging::init_logging(&data_dir)
                .unwrap_or_else(|_| data_dir.join(logging::LOG_DB_FILENAME));
            let db = el_db::Database::open(&data_dir.join("endlessli.db"))
                .map_err(|err| format!("open database: {err}"))?;

            let (sidecar_tx, mut sidecar_rx) = tokio::sync::mpsc::unbounded_channel();
            let sidecar = sidecar::SidecarManager::new(sidecar_tx, data_dir.join("models"));
            let (research_tx, mut research_rx) =
                tokio::sync::mpsc::unbounded_channel::<el_research::ResearchEvent>();

            #[cfg(feature = "analytics")]
            let analytics = analytics::open(&data_dir);

            let app_state = state::AppState {
                db,
                sidecar,
                research: state::ResearchRegistry::default(),
                research_tx,
                embed: state::EmbedQueue::default(),
                #[cfg(feature = "analytics")]
                analytics,
                data_dir,
                log_db_path,
            };
            app.manage(app_state.clone());

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(status) = sidecar_rx.recv().await {
                    let _ = app_handle.emit("sidecar://status", format!("{status:?}"));
                }
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = research_rx.recv().await {
                    forward_research_event(&app_handle, event);
                }
            });

            // Keeps note vectors current; started here so it outlives commands.
            let worker_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                embedding::run_worker(worker_handle, app_state).await;
            });

            tracing::info!("EndlessLi starting");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Maps orchestration progress onto the typed research events.
fn forward_research_event(app: &tauri::AppHandle, event: el_research::ResearchEvent) {
    use el_research::ResearchEventKind;
    match event.kind {
        ResearchEventKind::Step(step) => {
            let _ = research::ResearchStepEvent {
                session_id: event.session_id,
                step,
            }
            .emit(app);
        }
        ResearchEventKind::Result(result) => {
            let _ = research::ResearchResultEvent {
                session_id: event.session_id,
                result,
            }
            .emit(app);
        }
        ResearchEventKind::Status(status) => {
            let _ = research::ResearchStatusEvent {
                session_id: event.session_id,
                status,
            }
            .emit(app);
        }
    }
}
