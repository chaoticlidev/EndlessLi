//! Library commands (Phase 9): archived sources, a managed file store,
//! full-text search with type/tag/source filters, and note linking.
//!
//! Trust boundary: an imported file's original path is only read once, to
//! copy its bytes into `<data_dir>/library/`. From then on the app works
//! exclusively with the managed copy (the `file_path` column), so a file
//! moved, replaced, or deleted behind our back can never redirect a read.

use std::path::{Path, PathBuf};

use el_core::{
    AppError, AppResult, LibraryItem, NewLibraryItem, NewNote, Note, Tag, UpdateLibraryItem,
};
use el_db::repo::{library as library_repo, notes as note_repo, research as research_repo};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::state::AppState;

/// Hard cap on a file we will import (bytes).
const MAX_IMPORT_BYTES: u64 = 200 * 1024 * 1024;
/// Files at or below this size are read as text for the search index.
const MAX_TEXT_BYTES: u64 = 10 * 1024 * 1024;
/// Upper bound on stored extracted text (chars) to keep the database sane.
const MAX_CONTENT_CHARS: usize = 1_000_000;

/// A library item with its tags, for the detail drawer.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LibraryItemDetail {
    pub item: LibraryItem,
    pub tags: Vec<Tag>,
}

/// One row of the Library categories panel.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LibraryTypeCount {
    pub item_type: String,
    #[specta(type = specta_typescript::Number)]
    pub count: i64,
}

/// Item totals for the Library categories panel.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LibraryStats {
    #[specta(type = specta_typescript::Number)]
    pub total: i64,
    pub by_type: Vec<LibraryTypeCount>,
}

/// Outcome of an import: items created/kept plus per-file error messages.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LibraryImportResult {
    pub items: Vec<LibraryItem>,
    pub errors: Vec<String>,
}

/// Managed directory that holds copies of every imported file.
fn library_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("library")
}

/// Lists items with optional type, tag, and source filters (no query).
#[tauri::command]
#[specta::specta]
pub fn library_list(
    state: State<'_, AppState>,
    item_type: Option<String>,
    tag_id: Option<String>,
    source: Option<String>,
    limit: u32,
) -> AppResult<Vec<LibraryItem>> {
    let conn = state.db.connect()?;
    library_repo::search_filtered(
        &conn,
        None,
        item_type.as_deref(),
        tag_id.as_deref(),
        source.as_deref(),
        limit as usize,
    )
}

/// Full-text searches titles and extracted content, with the same filters.
#[tauri::command]
#[specta::specta]
pub fn library_search(
    state: State<'_, AppState>,
    query: String,
    item_type: Option<String>,
    tag_id: Option<String>,
    source: Option<String>,
    limit: u32,
) -> AppResult<Vec<LibraryItem>> {
    let conn = state.db.connect()?;
    library_repo::search_filtered(
        &conn,
        Some(&query),
        item_type.as_deref(),
        tag_id.as_deref(),
        source.as_deref(),
        limit as usize,
    )
}

/// Fetches an item with its tags.
#[tauri::command]
#[specta::specta]
pub fn library_get(state: State<'_, AppState>, id: String) -> AppResult<Option<LibraryItemDetail>> {
    let conn = state.db.connect()?;
    let Some(item) = library_repo::get(&conn, &id)? else {
        return Ok(None);
    };
    let tags = library_repo::tags_for_item(&conn, &id)?;
    Ok(Some(LibraryItemDetail { item, tags }))
}

/// Creates a library item from user-entered metadata.
#[tauri::command]
#[specta::specta]
pub fn library_create(state: State<'_, AppState>, input: NewLibraryItem) -> AppResult<LibraryItem> {
    let conn = state.db.connect()?;
    library_repo::create(&conn, &input)
}

/// Updates an item's editable metadata. `None` fields are left unchanged.
#[tauri::command]
#[specta::specta]
pub fn library_update(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateLibraryItem,
) -> AppResult<LibraryItem> {
    let conn = state.db.connect()?;
    library_repo::update(&conn, &id, &patch)
}

/// Deletes an item and its managed file copy (never the original path).
#[tauri::command]
#[specta::specta]
pub fn library_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.connect()?;
    let item = library_repo::require(&conn, &id)?;
    library_repo::delete(&conn, &id)?;
    drop(conn);
    if let Some(file_path) = item.file_path.as_deref() {
        remove_managed_file(&state.data_dir, file_path);
    }
    Ok(())
}

/// Totals for the categories panel: overall count plus per-type counts.
#[tauri::command]
#[specta::specta]
pub fn library_stats(state: State<'_, AppState>) -> AppResult<LibraryStats> {
    let conn = state.db.connect()?;
    let counts = library_repo::type_counts(&conn)?;
    let total = counts.iter().map(|(_, count)| *count).sum();
    Ok(LibraryStats {
        total,
        by_type: counts
            .into_iter()
            .map(|(item_type, count)| LibraryTypeCount { item_type, count })
            .collect(),
    })
}

/// Distinct source names for the categories panel filter.
#[tauri::command]
#[specta::specta]
pub fn library_sources(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    let conn = state.db.connect()?;
    library_repo::sources(&conn)
}

/// Attaches a tag to an item. Repeat calls are harmless.
#[tauri::command]
#[specta::specta]
pub fn library_assign_tag(
    state: State<'_, AppState>,
    item_id: String,
    tag_id: String,
) -> AppResult<()> {
    let conn = state.db.connect()?;
    library_repo::assign_tag(&conn, &item_id, &tag_id)
}

/// Detaches a tag from an item.
#[tauri::command]
#[specta::specta]
pub fn library_unassign_tag(
    state: State<'_, AppState>,
    item_id: String,
    tag_id: String,
) -> AppResult<()> {
    let conn = state.db.connect()?;
    library_repo::unassign_tag(&conn, &item_id, &tag_id)
}

/// Links an item to an existing note, or unlinks it with `None`.
#[tauri::command]
#[specta::specta]
pub fn library_link_note(
    state: State<'_, AppState>,
    item_id: String,
    note_id: Option<String>,
) -> AppResult<LibraryItem> {
    let conn = state.db.connect()?;
    if let Some(note_id) = note_id.as_deref() {
        // Fail with a clear message before the foreign key would.
        note_repo::require(&conn, note_id)?;
    }
    library_repo::set_note(&conn, &item_id, note_id.as_deref())
}

/// Creates an `import`-authored note from an item's metadata and text, then
/// links the two. The item keeps its own file copy.
#[tauri::command]
#[specta::specta]
pub fn library_create_note(
    state: State<'_, AppState>,
    item_id: String,
    project_id: Option<String>,
    folder_id: Option<String>,
) -> AppResult<Note> {
    let conn = state.db.connect()?;
    let item = library_repo::require(&conn, &item_id)?;
    let note = note_repo::create(
        &conn,
        &NewNote {
            title: item.title.clone(),
            content: note_content_from_item(&item),
            project_id,
            folder_id,
            author: "import".to_string(),
            source_session_id: None,
            created_from_note_id: None,
        },
    )?;
    library_repo::set_note(&conn, &item_id, Some(&note.id))?;
    drop(conn);
    state.embed.enqueue(note.id.clone());
    Ok(note)
}

/// Opens the native file picker and returns the chosen absolute paths.
/// Returns an empty list when the user cancels.
#[tauri::command]
#[specta::specta]
pub async fn library_pick_files(app: AppHandle) -> AppResult<Vec<String>> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Import files into the Library")
            .add_filter(
                "Documents",
                &[
                    "pdf", "md", "markdown", "txt", "rst", "epub", "csv", "tsv", "json", "html",
                    "htm",
                ],
            )
            .add_filter("All files", &["*"])
            .blocking_pick_files()
    })
    .await
    .map_err(|err| AppError::Internal(format!("file dialog task: {err}")))?;
    let Some(files) = picked else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::with_capacity(files.len());
    for file in files {
        match file.into_path() {
            Ok(path) => paths.push(path.to_string_lossy().to_string()),
            Err(err) => tracing::warn!("discarding unusable picked path: {err}"),
        }
    }
    Ok(paths)
}

/// Copies the given files into the managed library directory, extracts text,
/// and creates one item per file. Already-imported paths still copy (each
/// import is a distinct item); failures are reported per file, not fatal.
#[tauri::command]
#[specta::specta]
pub async fn library_import_files(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> AppResult<LibraryImportResult> {
    if paths.is_empty() {
        return Err(AppError::InvalidInput("no files were provided".to_string()));
    }
    let state = state.inner().clone();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        match import_one(&state, &path).await {
            Ok(item) => items.push(item),
            Err(err) => errors.push(format!("{path}: {err}")),
        }
    }
    Ok(LibraryImportResult { items, errors })
}

/// Sends selected research source results to the Library in one action.
/// Full page text is reused from the research clip cache when available,
/// falling back to the stored snippet. Items with a known URL are deduped.
#[tauri::command]
#[specta::specta]
pub async fn library_add_research_sources(
    state: State<'_, AppState>,
    session_id: String,
    result_ids: Vec<String>,
) -> AppResult<Vec<LibraryItem>> {
    if result_ids.is_empty() {
        return Err(AppError::InvalidInput(
            "select at least one source".to_string(),
        ));
    }
    let state = state.inner().clone();
    let results = {
        let conn = state.db.connect()?;
        research_repo::require_session(&conn, &session_id)?;
        research_repo::list_results(&conn, &session_id)?
    };
    let chosen: Vec<_> = results
        .into_iter()
        .filter(|result| result.result_type == "source" && result_ids.contains(&result.id))
        .collect();
    if chosen.is_empty() {
        return Err(AppError::InvalidInput(
            "none of the selected results are sources of this session".to_string(),
        ));
    }

    let mut items = Vec::new();
    for result in chosen {
        let url = result
            .source_url
            .clone()
            .filter(|value| !value.trim().is_empty());
        // Reuse an existing item for the same URL instead of duplicating.
        if let Some(url) = url.as_deref() {
            let conn = state.db.connect()?;
            if let Some(existing) = library_repo::get_by_url(&conn, url)? {
                items.push(existing);
                continue;
            }
        }
        let mut content = Some(result.content.clone());
        if let Some(url) = url.as_deref() {
            if let Ok(text) = clip_cached(&state, url).await {
                if !text.trim().is_empty() {
                    content = Some(text);
                }
            }
        }
        let title = result
            .source_title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| url.clone())
            .unwrap_or_else(|| "Untitled source".to_string());
        let source = url.as_deref().and_then(host_of);
        let conn = state.db.connect()?;
        let item = library_repo::create(
            &conn,
            &NewLibraryItem {
                item_type: "article".to_string(),
                title,
                authors: None,
                url,
                source,
                publication_date: None,
                content,
                file_path: None,
            },
        )?;
        items.push(item);
    }
    Ok(items)
}

/// Copies one untrusted file into the managed store and records an item.
async fn import_one(state: &AppState, raw_path: &str) -> AppResult<LibraryItem> {
    let canonical = std::fs::canonicalize(raw_path)
        .map_err(|err| AppError::InvalidInput(format!("cannot read file: {err}")))?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|err| AppError::InvalidInput(format!("cannot read file: {err}")))?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput("not a regular file".to_string()));
    }
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(AppError::InvalidInput(format!(
            "file is larger than {} MB",
            MAX_IMPORT_BYTES / (1024 * 1024)
        )));
    }

    let ext = canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let item_type = item_type_from_ext(&ext);
    let stem = canonical
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    let dir = library_dir(&state.data_dir);
    std::fs::create_dir_all(&dir)
        .map_err(|err| AppError::Internal(format!("create library dir: {err}")))?;
    // A fresh generated name (plus a sanitized extension) means no part of
    // the user's path influences the destination: no traversal, no collision.
    let file_id = uuid::Uuid::now_v7().to_string();
    let safe_ext: String = ext
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    let file_name = if safe_ext.is_empty() {
        file_id
    } else {
        format!("{file_id}.{safe_ext}")
    };
    let dest = dir.join(file_name);
    std::fs::copy(&canonical, &dest)
        .map_err(|err| AppError::Internal(format!("copy into library: {err}")))?;

    let mut content = None;
    let mut authors = None;
    let mut title = stem;
    if item_type == "pdf" {
        match extract_pdf_text(state, &dest).await {
            Ok((text, pdf_title, pdf_author)) => {
                content = text;
                if let Some(value) = pdf_title.filter(|value| !value.trim().is_empty()) {
                    title = value;
                }
                if let Some(value) = pdf_author.filter(|value| !value.trim().is_empty()) {
                    authors = serde_json::to_string(&[value]).ok();
                }
            }
            Err(err) => tracing::warn!("pdf text extraction skipped: {err}"),
        }
    } else if is_text_ext(&ext) && metadata.len() <= MAX_TEXT_BYTES {
        if let Ok(text) = std::fs::read_to_string(&dest) {
            content = Some(truncate_chars(text, MAX_CONTENT_CHARS));
        }
    }

    let conn = state.db.connect()?;
    library_repo::create(
        &conn,
        &NewLibraryItem {
            item_type: item_type.to_string(),
            title,
            authors,
            url: None,
            source: Some("Local file".to_string()),
            publication_date: None,
            content,
            file_path: Some(dest.to_string_lossy().to_string()),
        },
    )
}

/// Extracts PDF text and metadata through the sidecar. A missing sidecar is
/// non-fatal: the item is still imported, just without indexed text.
async fn extract_pdf_text(
    state: &AppState,
    path: &Path,
) -> AppResult<(Option<String>, Option<String>, Option<String>)> {
    let response = state
        .sidecar
        .call(
            "pdf.extract",
            serde_json::json!({ "path": path.to_string_lossy() }),
        )
        .await?;
    let text = response
        .get("text")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| truncate_chars(value.to_string(), MAX_CONTENT_CHARS));
    let metadata = response.get("metadata");
    let title = metadata
        .and_then(|value| value.get("title"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let author = metadata
        .and_then(|value| value.get("author"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    Ok((text, title, author))
}

/// Clips a URL through the shared webpage cache (a research run likely
/// already populated it, so this is normally offline).
async fn clip_cached(state: &AppState, url: &str) -> AppResult<String> {
    let (bytes, _) = el_cache::get_or_compute(
        &state.db,
        el_cache::CacheInput {
            entry_type: "webpage",
            query: url.to_string(),
            model: "readability".to_string(),
            ttl_secs: None,
        },
        || async {
            let response = state
                .sidecar
                .call(
                    "clip.url",
                    serde_json::json!({ "url": url, "max_chars": 12_000 }),
                )
                .await?;
            Ok(el_cache::CacheValue {
                bytes: serde_json::to_vec(&response)
                    .map_err(|err| AppError::Internal(format!("encode clip response: {err}")))?,
                source_type: "clip".to_string(),
            })
        },
    )
    .await?;
    let response: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|err| AppError::Internal(format!("decode cached clip: {err}")))?;
    Ok(response
        .get("text")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string())
}

/// Maps a file extension to the closest library type. Unknown extensions
/// default to `article`.
fn item_type_from_ext(ext: &str) -> &'static str {
    match ext {
        "pdf" => "pdf",
        "epub" | "mobi" | "azw3" => "book",
        "mp3" | "m4a" | "wav" | "ogg" | "flac" => "podcast",
        "mp4" | "mov" | "mkv" | "webm" | "avi" => "video",
        "csv" | "tsv" | "parquet" | "json" | "xlsx" => "dataset",
        _ => "article",
    }
}

/// Extensions read as plain text for the search index.
fn is_text_ext(ext: &str) -> bool {
    matches!(
        ext,
        "md" | "markdown"
            | "txt"
            | "rst"
            | "html"
            | "htm"
            | "csv"
            | "tsv"
            | "json"
            | "xml"
            | "yaml"
            | "yml"
    )
}

/// Truncates a string to at most `max` characters, on a char boundary.
fn truncate_chars(mut value: String, max: usize) -> String {
    if value.chars().count() <= max {
        return value;
    }
    let end = value
        .char_indices()
        .nth(max)
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    value.truncate(end);
    value
}

/// Builds note Markdown from an item: title, metadata, URL, then the text.
fn note_content_from_item(item: &LibraryItem) -> String {
    let mut content = String::new();
    if let Some(url) = item.url.as_deref() {
        content.push_str(&format!("Source: <{url}>\n\n"));
    }
    if let Some(authors) = item.authors.as_deref() {
        content.push_str(&format!("Authors: {}\n\n", authors_text(authors)));
    }
    if let Some(date) = item.publication_date.as_deref() {
        content.push_str(&format!("Published: {date}\n\n"));
    }
    if let Some(text) = item.content.as_deref() {
        content.push_str(text);
    }
    content.trim().to_string()
}

/// Renders a stored authors JSON array as a comma-separated string, falling
/// back to the raw value when it is not valid JSON.
fn authors_text(raw: &str) -> String {
    match serde_json::from_str::<Vec<String>>(raw) {
        Ok(names) => names.join(", "),
        Err(_) => raw.to_string(),
    }
}

/// Extracts a lowercase host from a URL for use as an item's source name.
/// Returns `None` for anything without a usable authority component.
fn host_of(url: &str) -> Option<String> {
    let authority = url.split("://").nth(1)?.split(['/', '?', '#']).next()?;
    let host = authority
        .rsplit('@')
        .next()?
        .split(':')
        .next()
        .unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Deletes a managed copy, but only when it lives inside the library
/// directory (defense in depth against a tampered `file_path`).
fn remove_managed_file(data_dir: &Path, file_path: &str) {
    // Canonicalize both sides: a tampered row like
    // `<library>/../../valuable.txt` would pass a component-wise prefix
    // check, and a symlink inside the library could point elsewhere.
    let Ok(library) = library_dir(data_dir).canonicalize() else {
        tracing::warn!(path = file_path, "library directory is unavailable");
        return;
    };
    let Ok(path) = Path::new(file_path).canonicalize() else {
        // Already gone (or never existed): nothing to delete.
        return;
    };
    if !path.starts_with(&library) {
        tracing::warn!(path = file_path, "refusing to delete non-managed path");
        return;
    }
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            tracing::warn!(path = file_path, "refusing to delete a non-regular file");
            return;
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            tracing::warn!(path = file_path, "managed file metadata failed: {err}");
            return;
        }
    }
    if let Err(err) = std::fs::remove_file(&path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = file_path, "managed file delete failed: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_mapping_and_truncation() {
        assert_eq!(item_type_from_ext("pdf"), "pdf");
        assert_eq!(item_type_from_ext("epub"), "book");
        assert_eq!(item_type_from_ext("mp3"), "podcast");
        assert_eq!(item_type_from_ext("mp4"), "video");
        assert_eq!(item_type_from_ext("csv"), "dataset");
        assert_eq!(item_type_from_ext("md"), "article");
        assert_eq!(item_type_from_ext(""), "article");
        assert!(is_text_ext("markdown"));
        assert!(!is_text_ext("pdf"));

        assert_eq!(truncate_chars("hello".to_string(), 10), "hello");
        assert_eq!(truncate_chars("héllo".to_string(), 3), "hél");
        // Never splits a multi-byte character.
        assert_eq!(truncate_chars("日本語".to_string(), 2), "日本");
    }

    #[test]
    fn authors_render_from_json_or_raw() {
        assert_eq!(authors_text(r#"["Ada","Alan"]"#), "Ada, Alan");
        assert_eq!(authors_text("not json"), "not json");
    }

    #[test]
    fn host_is_extracted_from_urls() {
        assert_eq!(
            host_of("https://en.wikipedia.org/wiki/Rust"),
            Some("en.wikipedia.org".to_string())
        );
        assert_eq!(
            host_of("http://user@Example.COM:8080/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            host_of("https://example.com?x=1"),
            Some("example.com".to_string())
        );
        assert_eq!(host_of("not a url"), None);
    }
}
