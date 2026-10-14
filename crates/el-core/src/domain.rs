//! Domain entities: one struct per table in the v1 schema.
//!
//! These types are the shared vocabulary for every `el-*` crate and, from
//! Phase 2, the source for generated TypeScript types. Timestamps are Unix
//! epoch seconds. Constrained string columns (`mode`, `status`, `author`,
//! ...) are validated in `el-db` against the `*_VALUES` lists below, which
//! mirror the SQL `CHECK` constraints.
//!
//! Integer policy for TypeScript export: every `i64` field carries
//! `#[specta(type = specta_typescript::Number)]`. All values crossing IPC
//! (timestamps, counters, token counts) stay far below 2^53, so they map to
//! JS `number` exactly. If a field ever needs true 64-bit range, give it a
//! dedicated string-serialized type instead of widening this policy.

use serde::{Deserialize, Serialize};

/// Allowed values for `notes.author`.
pub const NOTE_AUTHOR_VALUES: &[&str] = &["user", "ai", "import"];
/// Allowed values for `research_sessions.mode`.
pub const RESEARCH_MODE_VALUES: &[&str] = &["light", "deep"];
/// Allowed values for `research_sessions.status`.
pub const RESEARCH_STATUS_VALUES: &[&str] =
    &["pending", "in_progress", "completed", "failed", "cancelled"];
/// Allowed values for `research_steps.status`.
pub const STEP_STATUS_VALUES: &[&str] = &["pending", "in_progress", "completed", "failed"];
/// Allowed values for `research_results.type`.
pub const RESULT_TYPE_VALUES: &[&str] = &["summary", "source", "insight", "citation"];
/// Allowed values for `memory_entries.type`.
pub const MEMORY_TYPE_VALUES: &[&str] = &["global", "project"];
/// Allowed values for `memory_entries.source`.
pub const MEMORY_SOURCE_VALUES: &[&str] = &["user", "ai_suggested"];
/// Allowed values for `library_items.type`.
pub const LIBRARY_TYPE_VALUES: &[&str] = &[
    "pdf", "article", "book", "video", "dataset", "podcast", "paper",
];
/// Allowed values for `note_links.origin`.
pub const LINK_ORIGIN_VALUES: &[&str] = &["manual", "auto_approved"];
/// Allowed values for `cache_entries.type`.
pub const CACHE_TYPE_VALUES: &[&str] = &["search", "llm_response", "webpage", "embedding"];

/// A named collection of notes with its own memory scope and settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub system_prompt: Option<String>,
    pub encrypted: bool,
    pub archived: bool,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
}

/// Fields accepted when creating a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct NewProject {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub system_prompt: Option<String>,
}

/// Fields that may change on project update. `None` means "leave as is"
/// (same JSON reasoning as `UpdateNote`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub system_prompt: Option<String>,
}

/// A folder inside a project. `parent_id` enables nesting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
}

/// Fields accepted when creating a folder.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewFolder {
    pub name: String,
    pub project_id: String,
    pub parent_id: Option<String>,
}

/// A case-insensitively unique label attachable to notes and library items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

/// A Markdown note. `project_id` is `None` for Inbox notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub project_id: Option<String>,
    pub folder_id: Option<String>,
    pub author: String,
    pub source_session_id: Option<String>,
    pub created_from_note_id: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_hash: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub updated_at: i64,
}

/// Fields accepted when creating a note.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct NewNote {
    pub title: String,
    pub content: String,
    pub project_id: Option<String>,
    pub folder_id: Option<String>,
    pub author: String,
    pub source_session_id: Option<String>,
    pub created_from_note_id: Option<String>,
}

/// Fields that may change on note update. `None` means "leave as is".
/// Single `Option` (never nested): over JSON, `null` deserializes to `None`,
/// so absence and explicit null both mean "unchanged". Placement changes use
/// `note_move` instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct UpdateNote {
    pub title: Option<String>,
    pub content: Option<String>,
}

/// A bidirectional link between two notes, stored once with `note_a < note_b`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct NoteLink {
    pub note_a: String,
    pub note_b: String,
    pub similarity: Option<f64>,
    pub origin: String,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
}

/// One research run: a query plus its lifecycle state and token totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ResearchSession {
    pub id: String,
    pub query: String,
    pub mode: String,
    pub status: String,
    pub project_id: Option<String>,
    pub folder_id: Option<String>,
    pub context_memory: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub tokens_in: i64,
    #[specta(type = specta_typescript::Number)]
    pub tokens_out: i64,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub completed_at: Option<i64>,
}

/// Fields accepted when creating a research session.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewSession {
    pub query: String,
    pub mode: String,
    pub project_id: Option<String>,
    pub folder_id: Option<String>,
    pub context_memory: Option<String>,
}

/// A single step in a session, optionally nested for Deep Dive recursion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ResearchStep {
    pub id: String,
    pub session_id: String,
    pub parent_step_id: Option<String>,
    pub description: String,
    pub status: String,
    #[specta(type = specta_typescript::Number)]
    pub step_order: i64,
    pub details: Option<String>,
}

/// Fields accepted when recording a step.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewStep {
    pub session_id: String,
    pub parent_step_id: Option<String>,
    pub description: String,
    pub status: String,
    pub details: Option<String>,
}

/// One output row of a session: summary, source, insight, or citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ResearchResult {
    pub id: String,
    pub session_id: String,
    pub result_type: String,
    pub content: String,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub result_order: i64,
    pub selected_for_note: bool,
}

/// Fields accepted when recording a result.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewResult {
    pub session_id: String,
    pub result_type: String,
    pub content: String,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
}

/// A durable memory: global, or scoped to one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: String,
    pub project_id: Option<String>,
    pub content: String,
    pub source: String,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub expires_at: Option<i64>,
    pub is_active: bool,
}

/// Fields accepted when creating a memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewMemoryEntry {
    pub memory_type: String,
    pub project_id: Option<String>,
    pub content: String,
    pub source: String,
    #[specta(type = specta_typescript::Number)]
    pub expires_at: Option<i64>,
}

/// An AI-suggested memory awaiting user approval. Never used directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct MemorySuggestion {
    pub id: String,
    pub project_id: Option<String>,
    pub content: String,
    pub session_id: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
}

/// An archived source: PDF, article, book, video, dataset, podcast, or paper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct LibraryItem {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub authors: Option<String>,
    pub url: Option<String>,
    pub source: Option<String>,
    pub publication_date: Option<String>,
    pub content: Option<String>,
    pub file_path: Option<String>,
    pub note_id: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
}

/// Fields that may change on library item update. `None` means "leave as is"
/// (same JSON reasoning as `UpdateNote`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct UpdateLibraryItem {
    pub title: Option<String>,
    pub authors: Option<String>,
    pub url: Option<String>,
    pub source: Option<String>,
    pub content: Option<String>,
    pub file_path: Option<String>,
    pub note_id: Option<String>,
}

/// Fields accepted when creating a library item.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewLibraryItem {
    pub item_type: String,
    pub title: String,
    pub authors: Option<String>,
    pub url: Option<String>,
    pub source: Option<String>,
    pub publication_date: Option<String>,
    pub content: Option<String>,
    pub file_path: Option<String>,
}

/// A reusable Markdown prompt template with YAML frontmatter and parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub parameters: String,
    pub category: String,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub updated_at: i64,
}

/// Fields accepted when creating or replacing a skill.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewSkill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub parameters: String,
    pub category: String,
}

/// A reusable Python automation with sandbox metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Script {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub parameters: String,
    pub category: String,
    pub allow_network: bool,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub updated_at: i64,
}

/// Fields accepted when creating or replacing a script.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct NewScript {
    pub name: String,
    pub description: String,
    pub content: String,
    pub parameters: String,
    pub category: String,
    pub allow_network: bool,
}

/// One cached response, keyed by SHA-256 of (type, model, normalized query).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CacheEntry {
    pub id: String,
    pub entry_type: String,
    pub query: String,
    pub response: Vec<u8>,
    pub source_type: String,
    #[specta(type = specta_typescript::Number)]
    pub size_bytes: i64,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub last_accessed_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub access_count: i64,
    #[specta(type = specta_typescript::Number)]
    pub expires_at: Option<i64>,
}

/// Cache usage totals for the Settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
pub struct CacheStats {
    #[specta(type = specta_typescript::Number)]
    pub entries: i64,
    #[specta(type = specta_typescript::Number)]
    pub bytes: i64,
}

/// One logged LLM call for budget accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TokenUsage {
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
    pub session_id: Option<String>,
    pub provider: String,
    pub model: String,
    #[specta(type = specta_typescript::Number)]
    pub tokens_in: i64,
    #[specta(type = specta_typescript::Number)]
    pub tokens_out: i64,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
}

/// A single key/value setting. Values hold JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

/// A vector-similarity candidate for auto-linking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SimilarNote {
    pub note_id: String,
    /// Cosine distance (0.0 is identical) from the sqlite-vec cosine metric.
    pub distance: f64,
}
