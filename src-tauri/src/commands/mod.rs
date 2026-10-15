//! Thin Tauri IPC handlers (one module per area).
//!
//! Handlers validate input, call into `el-*` crates, and map errors to
//! [`el_core::AppError`]. No business logic lives here, which keeps handlers
//! unit-testable and the IPC surface auditable. Every command carries
//! `#[specta::specta]` so `src/ipc/bindings.ts` is generated from this
//! module list; nothing on the frontend is hand-copied from Rust.

pub mod folders;
pub mod links;
pub mod notes;
pub mod projects;
pub mod secrets;
pub mod settings;
pub mod tags;
