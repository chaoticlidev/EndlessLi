//! EndlessLi Tauri shell: app lifecycle, IPC wiring, and sidecar ownership.
//!
//! `main.rs` stays a two-line entry point while this library owns setup
//! (logging, database, state, plugins). Command handlers live in
//! [`commands`] and stay thin; domain logic lives in `crates/el-*`.
//! TypeScript bindings for every command are generated into
//! `src/ipc/bindings.ts` on debug builds and in the export test below.

pub mod commands;
pub mod logging;
pub mod sidecar;
pub mod state;

use specta_typescript::Typescript;
use tauri::Manager;
use tauri_specta::{collect_commands, Builder};

use commands::{folders, links, notes, projects, tags};

/// The full IPC surface, exposed for the bindings export test.
/// Both `run` and codegen build from this one list so bindings can never
/// drift from the handler registration.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
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
    ])
}

/// Bootstraps logging, the database, shared state, and plugins, then runs.
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
            app.manage(state::AppState {
                db,
                data_dir,
                log_db_path,
            });
            tracing::info!("EndlessLi starting");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
