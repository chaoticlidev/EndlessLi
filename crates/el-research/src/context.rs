//! Prompt context assembly from user-controlled memory.
//!
//! Phase 8 owns the full hierarchy (session overrides, suggestion queue,
//! expiry UI). Until then this module assembles the read-only block every
//! LLM call receives: active global entries, then active project entries,
//! then caller-supplied session context. The global kill-switch empties it.

use el_core::AppResult;
use rusqlite::Connection;

/// Builds the memory block for prompts. Empty string when memory is disabled
/// or nothing is stored.
pub fn build_context(
    conn: &Connection,
    project_id: Option<&str>,
    session_memory: Option<&str>,
) -> AppResult<String> {
    if !el_db::repo::memory::is_enabled(conn)? {
        return Ok(String::new());
    }
    let mut sections = Vec::new();
    let global = el_db::repo::memory::list_global(conn)?;
    if !global.is_empty() {
        let lines: Vec<String> = global
            .iter()
            .map(|entry| format!("- {}", entry.content))
            .collect();
        sections.push(format!("[Global memory]\n{}", lines.join("\n")));
    }
    if let Some(project_id) = project_id {
        let scoped = el_db::repo::memory::list_for_project(conn, project_id)?;
        if !scoped.is_empty() {
            let lines: Vec<String> = scoped
                .iter()
                .map(|entry| format!("- {}", entry.content))
                .collect();
            sections.push(format!("[Project memory]\n{}", lines.join("\n")));
        }
    }
    if let Some(memory) = session_memory {
        let trimmed = memory.trim();
        if !trimmed.is_empty() {
            sections.push(format!("[Session context]\n{trimmed}"));
        }
    }
    Ok(sections.join("\n\n"))
}
