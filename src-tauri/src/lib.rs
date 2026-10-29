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
    analytics as analytics_commands, audio, cache, compression as compression_commands,
    embedding as embedding_commands, folders, library, links, memory as memory_commands, notes,
    projects, provider as provider_commands, research, secrets, settings, skills, synthesis, tags,
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
            projects::project_encryption_set,
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
            library::library_list,
            library::library_search,
            library::library_get,
            library::library_create,
            library::library_update,
            library::library_delete,
            library::library_stats,
            library::library_sources,
            library::library_assign_tag,
            library::library_unassign_tag,
            library::library_link_note,
            library::library_create_note,
            library::library_pick_files,
            library::library_import_files,
            library::library_add_research_sources,
            memory_commands::memory_list,
            memory_commands::memory_list_forgotten,
            memory_commands::memory_create,
            memory_commands::memory_update,
            memory_commands::memory_set_scope,
            memory_commands::memory_forget,
            memory_commands::memory_restore,
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
            provider_commands::llm_catalog,
            provider_commands::llm_config_get,
            provider_commands::llm_config_save,
            provider_commands::llm_models_sync,
            audio::tts_config_get,
            audio::tts_config_save,
            audio::tts_voices,
            audio::tts_synthesize,
            cache::cache_stats,
            cache::cache_clear,
            research::research_start,
            research::research_cancel,
            research::research_delete,
            research::research_get,
            research::research_history,
            research::research_rename,
            research::research_save_results,
            research::sidecar_status,
            compression_commands::compression_start,
            compression_commands::compression_save_outputs,
            synthesis::synthesis_start,
            synthesis::synthesis_status,
            synthesis::synthesis_get,
            synthesis::synthesis_jobs,
            synthesis::synthesis_cancel,
            synthesis::synthesis_delete,
            synthesis::synthesis_save_output,
            skills::skill_list,
            skills::skill_get,
            skills::skill_parse,
            skills::skill_save,
            skills::skill_delete,
            skills::skill_run,
            skills::skill_save_output,
            skills::script_list,
            skills::script_get,
            skills::script_parse,
            skills::script_save,
            skills::script_delete,
            skills::script_run,
            skills::script_runtime,
            skills::script_save_output,
        ])
        .events(collect_events![
            research::ResearchStepEvent,
            research::ResearchResultEvent,
            research::ResearchStatusEvent,
            links::LinkingSuggestionsEvent,
            synthesis::SynthesisProgressEvent,
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state = window.state::<state::AppState>();
                state.kill_sidecar();
            }
        })
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            // Fail closed: never create the database in the process working
            // directory (which might be read-only, or somewhere unexpected).
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| std::io::Error::other(format!("resolve app data dir: {err}")))?;
            let log_db_path = logging::init_logging(&data_dir)
                .unwrap_or_else(|_| data_dir.join(logging::LOG_DB_FILENAME));
            // Optional full-database encryption. The passphrase is
            // read only by a `sqlcipher` build; a normal build ignores it.
            #[cfg(feature = "sqlcipher")]
            let db_key = database_key();
            #[cfg(not(feature = "sqlcipher"))]
            let db_key: Option<String> = None;
            if db_key.is_some() {
                tracing::info!("opening database with SQLCipher");
            }
            let db =
                el_db::Database::open_with_key(&data_dir.join("endlessli.db"), db_key.as_deref())
                    .map_err(|err| format!("open database: {err}"))?;
            // Seed the shipped skills/scripts the first time each table is
            // empty; failures are logged, never fatal.
            match db.connect() {
                Ok(conn) => {
                    if let Err(err) = skills::seed_builtins(&conn) {
                        tracing::warn!("seeding built-in skills/scripts failed: {err}");
                    }
                }
                Err(err) => tracing::warn!("could not seed built-ins: {err}"),
            }

            // Cleanup stale jobs on startup
            if let Ok(conn) = db.connect() {
                let _ = el_db::repo::synthesis::fail_stale_jobs(&conn);
            }

            let (sidecar_tx, mut sidecar_rx) =
                tokio::sync::mpsc::unbounded_channel::<sidecar::SidecarStatus>();
            let sidecar = sidecar::SidecarManager::new(sidecar_tx, data_dir.join("models"));

            sidecar.set_app_handle(app.handle().clone());

            let (research_tx, mut research_rx) =
                tokio::sync::mpsc::unbounded_channel::<el_research::ResearchEvent>();

            #[cfg(feature = "analytics")]
            let analytics = analytics::open(&data_dir);

            let app_state = state::AppState {
                app_handle: app.handle().clone(),
                db,
                sidecar,
                research: state::ResearchRegistry::default(),
                research_tx,
                synthesis: state::SynthesisRegistry::default(),
                embed: state::EmbedQueue::default(),
                #[cfg(feature = "analytics")]
                analytics,
                data_dir,
                log_db_path,
            };

            if let Err(err) = secrets::migrate_legacy_fallbacks(&app_state) {
                tracing::warn!("legacy API key cleanup failed: {err}");
            }
            if let Err(err) = provider_commands::migrate_config(&app_state) {
                tracing::warn!("LLM settings migration failed: {err}");
            }

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

/// Resolves the optional SQLCipher passphrase for a `sqlcipher` build.
///
/// Dev/testing can set `ENDLESSLI_DB_KEY`; release builds read the keychain
/// entry provisioned at install/onboarding time. A missing key means the
/// database is opened unencrypted.
#[cfg(feature = "sqlcipher")]
fn database_key() -> Option<String> {
    if let Ok(key) = std::env::var("ENDLESSLI_DB_KEY") {
        if !key.trim().is_empty() {
            return Some(key);
        }
    }
    el_security::secrets::load_database_key().ok()
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
