//! `el-linking`: auto-linking orchestration over sqlite-vec, and note
//! embedding.
//!
//! Why this crate exists: suggesting links needs one write path that stores
//! each pair once (`note_a < note_b`), filters rejections, and applies the
//! similarity threshold (default 0.75). Keeping that logic here guarantees
//! no orphaned half-links regardless of which UI surface triggers it.
//!
//! Embedding is the input side of the same pipeline: a note's text is turned
//! into a vector by the sidecar (via [`Embedder`]), stored in `sqlite-vec`
//! tagged with the model and a content hash, and reused to query neighbors.
//! Orchestration is transport-agnostic so it is unit-testable with stub
//! embedders and no network; the Tauri shell implements [`Embedder`] over the
//! sidecar and owns the debounced background queue.

use std::future::Future;

use el_core::{AppError, AppResult, EmbeddingModelInfo, EmbeddingStatus, LinkCandidate, Note};
use el_db::repo::notes as note_repo;
use el_db::vec;

/// Default embedding model (384-dim, ONNX, downloaded on first use).
pub const DEFAULT_EMBEDDING_MODEL: &str = "sentence-transformers/all-MiniLM-L6-v2";

/// Default cosine-similarity threshold for link candidates.
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.75;

/// Maximum candidates returned per note.
pub const MAX_CANDIDATES: u32 = 5;

/// Settings key recording the active embedding model.
pub const SETTING_EMBEDDING_MODEL: &str = "embedding.model";

/// Settings key overriding the auto-linking similarity threshold.
pub const SETTING_LINKING_THRESHOLD: &str = "linking.threshold";

/// Character ceiling for the text handed to the embedder, bounding model
/// input regardless of note length.
pub const MAX_EMBED_CHARS: usize = 8_000;

/// Notes embedded per re-embed batch.
pub const EMBED_BATCH: usize = 32;

/// Text-to-vector capability used by the embedding pipeline. Implemented in
/// the Tauri shell over the sidecar's `embed.texts` / `embed.model_info`.
pub trait Embedder: Send + Sync {
    /// Resolves a model id to its dimension and availability.
    fn model_info<'a>(
        &'a self,
        model: &'a str,
    ) -> impl Future<Output = AppResult<EmbeddingModelInfo>> + Send + 'a;

    /// Embeds a batch of texts, returning one vector per input in order.
    fn embed<'a>(
        &'a self,
        texts: &'a [String],
        model: &'a str,
    ) -> impl Future<Output = AppResult<Vec<Vec<f32>>>> + Send + 'a;
}

/// Resolves the configured embedding model, falling back to the default.
pub fn resolve_model(raw: Option<&str>) -> String {
    match raw {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => DEFAULT_EMBEDDING_MODEL.to_string(),
    }
}

/// Parses a stored threshold, clamped to `[0, 1]`, falling back to default.
pub fn threshold_from_setting(raw: Option<&str>) -> f64 {
    raw.and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| (0.0..=1.0).contains(value))
        .unwrap_or(DEFAULT_SIMILARITY_THRESHOLD)
}

/// The exact text embedded for a note: title, a blank line, then content,
/// truncated to [`MAX_EMBED_CHARS`].
pub fn note_embed_text(note: &Note) -> String {
    let mut text = String::with_capacity(note.title.len() + note.content.len() + 2);
    text.push_str(&note.title);
    text.push_str("\n\n");
    text.push_str(&note.content);
    if text.chars().count() > MAX_EMBED_CHARS {
        text = text.chars().take(MAX_EMBED_CHARS).collect();
    }
    text
}

/// SHA-256 of the embedded text (hex). Stored on the note so an unchanged
/// note is never re-embedded.
pub fn content_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    el_db::repo::hex(&hasher.finalize())
}

/// Creates or reconciles the vector table for the model, returning its
/// resolved metadata. Errors when the model is unavailable so callers can
/// surface a clear diagnostic instead of silently embedding nothing.
pub async fn ensure_embeddings_ready<E: Embedder>(
    db: &el_db::Database,
    embedder: &E,
    model: &str,
) -> AppResult<EmbeddingModelInfo> {
    let info = embedder.model_info(model).await?;
    if !info.available {
        return Err(AppError::Sidecar(format!(
            "embedding model {} is not available in the sidecar",
            info.model
        )));
    }
    let mut conn = db.connect()?;
    let sync = vec::sync_embeddings_table(&mut conn, &info.model, info.dim as usize)?;
    if sync.recreated {
        tracing::info!(model = %info.model, "embeddings table (re)created; all notes need embedding");
    }
    Ok(info)
}

/// Embeds one note, skipping it when its stored model and content hash are
/// already current. Returns whether a vector was written.
pub async fn embed_note<E: Embedder>(
    db: &el_db::Database,
    embedder: &E,
    model: &str,
    note_id: &str,
) -> AppResult<bool> {
    let note = {
        let conn = db.connect()?;
        match note_repo::get(&conn, note_id)? {
            Some(note) => note,
            None => return Ok(false),
        }
    };
    let text = note_embed_text(&note);
    if text.trim().is_empty() {
        return Ok(false);
    }
    let hash = content_hash(&text);
    if note.embedding_model.as_deref() == Some(model)
        && note.embedding_hash.as_deref() == Some(hash.as_str())
    {
        return Ok(false);
    }
    let mut vectors = embedder.embed(std::slice::from_ref(&text), model).await?;
    let vector = vectors
        .pop()
        .ok_or_else(|| AppError::Sidecar("embedder returned no vectors".to_string()))?;
    let conn = db.connect()?;
    // `upsert_embedding` only writes when the vector length matches the
    // active dimension, so a model/setting mismatch surfaces as an error
    // rather than corrupting the table.
    vec::upsert_embedding(&conn, note_id, model, &hash, &vector)?;
    Ok(true)
}

/// Embeds up to `limit` notes that are missing a current vector (never
/// embedded, or embedded with a different model). Returns how many were
/// written. Stops at the first sidecar failure so the caller can retry.
pub async fn embed_pending<E: Embedder>(
    db: &el_db::Database,
    embedder: &E,
    model: &str,
    limit: usize,
) -> AppResult<usize> {
    let ids = {
        let conn = db.connect()?;
        note_repo::list_needing_embedding(&conn, model, limit)?
    };
    let mut written = 0;
    for id in ids {
        if embed_note(db, embedder, model, &id).await? {
            written += 1;
        }
    }
    Ok(written)
}

/// Returns the auto-linking candidates for a note: the top `limit` notes by
/// cosine similarity at or above `threshold`, excluding the note itself,
/// existing links, and past rejections. Empty when the note has no vector.
pub fn suggest_links(
    db: &el_db::Database,
    note_id: &str,
    model: &str,
    threshold: f64,
    limit: u32,
) -> AppResult<Vec<LinkCandidate>> {
    let conn = db.connect()?;
    let Some(query) = vec::read_embedding(&conn, note_id)? else {
        return Ok(Vec::new());
    };
    let max_distance = (1.0 - threshold).max(0.0);
    let similar = vec::find_similar(&conn, &query, note_id, model, max_distance, limit as usize)?;
    let mut out = Vec::with_capacity(similar.len());
    for hit in similar {
        if let Some(note) = note_repo::get(&conn, &hit.note_id)? {
            out.push(LinkCandidate {
                note_id: note.id,
                title: note.title,
                // sqlite-vec's cosine distance is 1 - similarity.
                similarity: (1.0 - hit.distance).clamp(0.0, 1.0),
            });
        }
    }
    Ok(out)
}

/// Builds the Settings embedding-coverage report.
pub fn embedding_status(
    db: &el_db::Database,
    model: &str,
    dim: u32,
    available: bool,
) -> AppResult<EmbeddingStatus> {
    let conn = db.connect()?;
    let (embedded, total) = note_repo::embedding_counts(&conn, model)?;
    Ok(EmbeddingStatus {
        model: model.to_string(),
        dim,
        available,
        embedded,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use el_core::NewNote;

    /// Deterministic embedder: maps text to a fixed-dimension vector by
    /// keyword presence, so similarity is meaningful without a model.
    struct StubEmbedder {
        dim: usize,
    }

    impl StubEmbedder {
        fn new(dim: usize) -> Self {
            Self { dim }
        }
    }

    impl Embedder for StubEmbedder {
        async fn model_info(&self, model: &str) -> AppResult<EmbeddingModelInfo> {
            Ok(EmbeddingModelInfo {
                model: model.to_string(),
                dim: self.dim as u32,
                available: true,
                is_default: false,
            })
        }

        async fn embed(&self, texts: &[String], _model: &str) -> AppResult<Vec<Vec<f32>>> {
            Ok(texts
                .iter()
                .map(|text| vector_for(text, self.dim))
                .collect())
        }
    }

    /// Axis-per-keyword vector; unrelated keywords give near-zero similarity.
    fn vector_for(text: &str, dim: usize) -> Vec<f32> {
        let lower = text.to_lowercase();
        let keywords = [
            "cats", "dogs", "space", "cooking", "music", "history", "math", "ocean",
        ];
        let mut vector = vec![0.0f32; dim];
        let mut hit = false;
        for (index, keyword) in keywords.iter().enumerate() {
            if lower.contains(keyword) {
                vector[index] = 1.0;
                hit = true;
            }
        }
        if !hit {
            vector[dim - 1] = 1.0;
        }
        let norm: f32 = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }

    fn note(db: &el_db::Database, title: &str, content: &str) -> String {
        let conn = db.connect().unwrap();
        note_repo::create(
            &conn,
            &NewNote {
                title: title.to_string(),
                content: content.to_string(),
                project_id: None,
                folder_id: None,
                author: "user".to_string(),
                source_session_id: None,
                created_from_note_id: None,
            },
        )
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn embed_note_stores_and_skips_unchanged() {
        let (db, path) = el_db::Database::open_ephemeral().unwrap();
        let embedder = StubEmbedder::new(8);
        ensure_embeddings_ready(&db, &embedder, "stub")
            .await
            .unwrap();

        let id = note(&db, "Cats", "All about cats and kittens.");
        assert!(embed_note(&db, &embedder, "stub", &id).await.unwrap());

        {
            let conn = db.connect().unwrap();
            let stored = note_repo::require(&conn, &id).unwrap();
            assert_eq!(stored.embedding_model.as_deref(), Some("stub"));
            assert!(stored.embedding_hash.is_some());
            assert!(vec::read_embedding(&conn, &id).unwrap().is_some());
        }

        // Unchanged note is skipped.
        assert!(!embed_note(&db, &embedder, "stub", &id).await.unwrap());

        // A change re-embeds.
        {
            let mut conn = db.connect().unwrap();
            note_repo::update(
                &mut conn,
                &id,
                &el_core::UpdateNote {
                    title: None,
                    content: Some("Now about dogs.".to_string()),
                },
            )
            .unwrap();
        }
        assert!(embed_note(&db, &embedder, "stub", &id).await.unwrap());

        drop(db);
        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn related_note_is_top_candidate_and_exclusions_hold() {
        let (db, path) = el_db::Database::open_ephemeral().unwrap();
        let embedder = StubEmbedder::new(8);
        ensure_embeddings_ready(&db, &embedder, "stub")
            .await
            .unwrap();

        let target = note(&db, "Cats", "A guide to cats.");
        let related = note(&db, "More cats", "Cats again, and kittens.");
        let _dogs = note(&db, "Dogs", "Everything about dogs.");
        let _space = note(&db, "Space", "Rockets and space travel.");
        for existing in note_repo::list_all(&db.connect().unwrap()).unwrap() {
            embed_note(&db, &embedder, "stub", &existing.id)
                .await
                .unwrap();
        }

        let candidates = suggest_links(&db, &target, "stub", 0.75, MAX_CANDIDATES).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].note_id, related);
        assert!(candidates[0].similarity > 0.99);

        // Approving the link removes it from future suggestions.
        {
            let conn = db.connect().unwrap();
            el_db::repo::links::approve(&conn, &target, &related, Some(0.99), "auto_approved")
                .unwrap();
        }
        let after = suggest_links(&db, &target, "stub", 0.75, MAX_CANDIDATES).unwrap();
        assert!(after.is_empty());

        drop(db);
        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn embed_pending_backfills_stale_model() {
        let (db, path) = el_db::Database::open_ephemeral().unwrap();
        let embedder = StubEmbedder::new(8);
        ensure_embeddings_ready(&db, &embedder, "stub")
            .await
            .unwrap();

        let a = note(&db, "Cats", "cats");
        let b = note(&db, "Dogs", "dogs");
        assert_ne!(a, b);
        let written = embed_pending(&db, &embedder, "stub", EMBED_BATCH)
            .await
            .unwrap();
        assert_eq!(written, 2);
        assert_eq!(
            embed_pending(&db, &embedder, "stub", EMBED_BATCH)
                .await
                .unwrap(),
            0
        );

        drop(db);
        std::fs::remove_file(path).ok();
    }
}
