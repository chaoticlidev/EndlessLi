//! Vector search over `sqlite-vec` (pinned exact version, see workspace deps).
//!
//! Embeddings live in the source-of-truth database in a `vec0` table whose
//! dimension is fixed at creation. Switching embedding models drops and
//! recreates the table and marks every note stale, so similarity results can
//! never mix vectors from different models.

use std::sync::OnceLock;

use el_core::{AppError, AppResult, SimilarNote};
use rusqlite::Connection;

/// Name of the vector table holding one row per embedded note.
pub const EMBEDDINGS_TABLE: &str = "note_embeddings";
/// Settings key recording which model produced the stored vectors.
pub const SETTING_EMBEDDING_MODEL: &str = "embedding.model";
/// Settings key recording the dimension of the stored vectors.
pub const SETTING_EMBEDDING_DIM: &str = "embedding.dim";

/// Process-wide registration result: the auto-extension must be registered
/// exactly once, before the first pooled connection opens.
static REGISTRATION: OnceLock<Result<(), String>> = OnceLock::new();

/// Registers the statically linked `sqlite-vec` extension for every current
/// and future connection in this process.
pub fn register_extension() -> Result<(), String> {
    REGISTRATION
        .get_or_init(|| {
            // SAFETY: only the symbol address is used. The `sqlite-vec`
            // crate links the extension statically and declares the
            // entrypoint with a unit signature, so the documented
            // integration casts its address to the entrypoint type that
            // `sqlite3_auto_extension` expects. The `OnceLock` runs this
            // exactly once, so no connection can observe a
            // half-registered extension.
            #[allow(unsafe_code)]
            unsafe {
                let entry: VecEntryPoint =
                    std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
                let rc = rusqlite::ffi::sqlite3_auto_extension(entry);
                if rc == 0 {
                    Ok(())
                } else {
                    Err(format!("sqlite3_auto_extension failed with code {rc}"))
                }
            }
        })
        .clone()
}

/// Extension entrypoint type expected by `sqlite3_auto_extension`.
type VecEntryPoint = Option<
    unsafe extern "C" fn(
        *mut rusqlite::ffi::sqlite3,
        *mut *mut std::os::raw::c_char,
        *const rusqlite::ffi::sqlite3_api_routines,
    ) -> std::os::raw::c_int,
>;

/// Outcome of reconciling the vector table with the active embedding model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingsSync {
    /// True when the table was recreated and every note needs re-embedding.
    pub recreated: bool,
}

/// Creates or reconciles the vector table for `(model, dim)`.
///
/// When the model or dimension differs from what is recorded in `settings`,
/// the table is dropped and recreated and all notes are marked stale
/// (`embedding_model`/`embedding_hash` cleared). Returns whether that reset
/// happened so callers can schedule a re-embed job.
pub fn sync_embeddings_table(
    conn: &mut Connection,
    model: &str,
    dim: usize,
) -> AppResult<EmbeddingsSync> {
    if dim == 0 || dim > 4096 {
        return Err(AppError::InvalidInput(format!("invalid dimension {dim}")));
    }
    let stored_model: Option<String> = setting_get(conn, SETTING_EMBEDDING_MODEL)?;
    let stored_dim: Option<String> = setting_get(conn, SETTING_EMBEDDING_DIM)?;
    let current = format!("{dim}");
    if stored_model.as_deref() == Some(model) && stored_dim.as_deref() == Some(&current) {
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {EMBEDDINGS_TABLE} \
             USING vec0(note_id TEXT PRIMARY KEY, embedding float[{dim}] distance_metric=cosine);"
        ))
        .map_err(|err| AppError::Database(err.to_string()))?;
        return Ok(EmbeddingsSync { recreated: false });
    }
    let tx = conn
        .transaction()
        .map_err(|err| AppError::Database(err.to_string()))?;
    tx.execute_batch(&format!(
        "DROP TABLE IF EXISTS {EMBEDDINGS_TABLE};
         CREATE VIRTUAL TABLE {EMBEDDINGS_TABLE} \
         USING vec0(note_id TEXT PRIMARY KEY, embedding float[{dim}] distance_metric=cosine);
         UPDATE notes SET embedding_model = NULL, embedding_hash = NULL;"
    ))
    .map_err(|err| AppError::Database(err.to_string()))?;
    setting_put(&tx, SETTING_EMBEDDING_MODEL, model)?;
    setting_put(&tx, SETTING_EMBEDDING_DIM, &current)?;
    tx.commit()
        .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(EmbeddingsSync { recreated: true })
}

/// Stores (replacing any previous) the embedding for a note and records the
/// model and content hash on the note row, atomically.
pub fn upsert_embedding(
    conn: &Connection,
    note_id: &str,
    model: &str,
    content_hash: &str,
    vector: &[f32],
) -> AppResult<()> {
    let expected = expected_dim(conn)?;
    if vector.len() != expected {
        return Err(AppError::InvalidInput(format!(
            "vector has {} dims, table expects {expected}",
            vector.len()
        )));
    }
    let affected = conn
        .execute(
            "UPDATE notes SET embedding_model = ?1, embedding_hash = ?2 WHERE id = ?3",
            (model, content_hash, note_id),
        )
        .map_err(|err| AppError::Database(err.to_string()))?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("note {note_id}")));
    }
    conn.execute(
        &format!("DELETE FROM {EMBEDDINGS_TABLE} WHERE note_id = ?1"),
        [note_id],
    )
    .map_err(|err| AppError::Database(err.to_string()))?;
    conn.execute(
        &format!("INSERT INTO {EMBEDDINGS_TABLE}(note_id, embedding) VALUES (?1, ?2)"),
        rusqlite::params![note_id, encode_vector(vector)],
    )
    .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(())
}

/// Removes a note's vector, if present. Does nothing when the embeddings
/// table was never initialized, so notes stay deletable before any
/// embedding model is configured.
pub fn delete_embedding(conn: &Connection, note_id: &str) -> AppResult<()> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [EMBEDDINGS_TABLE],
            |row| row.get(0),
        )
        .map_err(|err| AppError::Database(err.to_string()))?;
    if !exists {
        return Ok(());
    }
    conn.execute(
        &format!("DELETE FROM {EMBEDDINGS_TABLE} WHERE note_id = ?1"),
        [note_id],
    )
    .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(())
}

/// Reads a note's stored vector back out of the embeddings table, or `None`
/// when the note has no vector or the table was never initialized. Used by
/// the auto-linking engine to query for neighbors without re-embedding.
pub fn read_embedding(conn: &Connection, note_id: &str) -> AppResult<Option<Vec<f32>>> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [EMBEDDINGS_TABLE],
            |row| row.get(0),
        )
        .map_err(|err| AppError::Database(err.to_string()))?;
    if !exists {
        return Ok(None);
    }
    let mut stmt = conn
        .prepare(&format!(
            "SELECT embedding FROM {EMBEDDINGS_TABLE} WHERE note_id = ?1"
        ))
        .map_err(|err| AppError::Database(err.to_string()))?;
    let mut rows = stmt
        .query([note_id])
        .map_err(|err| AppError::Database(err.to_string()))?;
    match rows
        .next()
        .map_err(|err| AppError::Database(err.to_string()))?
    {
        Some(row) => {
            let bytes: Vec<u8> = row
                .get(0)
                .map_err(|err| AppError::Database(err.to_string()))?;
            Ok(Some(decode_vector(&bytes)))
        }
        None => Ok(None),
    }
}

/// Finds notes similar to `query`, excluding the note itself, existing links,
/// and rejected suggestions. Only notes embedded with `model` participate, so
/// results never mix embedding spaces. `max_distance` is a cosine-distance
/// ceiling (0.0 is identical); similarity 0.75 maps to distance 0.25.
pub fn find_similar(
    conn: &Connection,
    query: &[f32],
    exclude_id: &str,
    model: &str,
    max_distance: f64,
    limit: usize,
) -> AppResult<Vec<SimilarNote>> {
    let expected = expected_dim(conn)?;
    if query.len() != expected {
        return Err(AppError::InvalidInput(format!(
            "query has {} dims, table expects {expected}",
            query.len()
        )));
    }
    // Overscan: the KNN scan runs without `k` (full scan with distances),
    // so exclusions below cannot crowd out good candidates.
    let overscan = limit.saturating_mul(10).saturating_add(25).max(50);
    let mut stmt = conn
        .prepare(&format!(
            "WITH knn(note_id, distance) AS (
               SELECT note_id, distance FROM {EMBEDDINGS_TABLE}
               WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2
             )
             SELECT knn.note_id, knn.distance FROM knn
             JOIN notes n ON n.id = knn.note_id
             WHERE knn.note_id != ?3
               AND n.embedding_model = ?4
               AND knn.distance <= ?5
               AND NOT EXISTS (
                 SELECT 1 FROM note_links l
                 WHERE (l.note_a = ?3 AND l.note_b = knn.note_id)
                    OR (l.note_b = ?3 AND l.note_a = knn.note_id))
               AND NOT EXISTS (
                 SELECT 1 FROM link_rejections r
                 WHERE (r.note_a = ?3 AND r.note_b = knn.note_id)
                    OR (r.note_b = ?3 AND r.note_a = knn.note_id))
             ORDER BY knn.distance LIMIT ?6"
        ))
        .map_err(|err| AppError::Database(err.to_string()))?;
    let rows = stmt
        .query_map(
            rusqlite::params![
                encode_vector(query),
                overscan as i64,
                exclude_id,
                model,
                max_distance,
                limit as i64
            ],
            |row| {
                Ok(SimilarNote {
                    note_id: row.get(0)?,
                    distance: row.get(1)?,
                })
            },
        )
        .map_err(|err| AppError::Database(err.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|err| AppError::Database(err.to_string()))?);
    }
    Ok(out)
}

/// Reads the active vector dimension from `settings`.
fn expected_dim(conn: &Connection) -> AppResult<usize> {
    let raw: Option<String> = setting_get(conn, SETTING_EMBEDDING_DIM)?;
    match raw {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| AppError::Database(format!("invalid {SETTING_EMBEDDING_DIM}: {value}"))),
        None => Err(AppError::InvalidInput(
            "embeddings table not initialized; call sync_embeddings_table first".to_string(),
        )),
    }
}

/// Serializes floats in native endianness, matching what the extension reads.
fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
    bytes
}

/// Reverses [`encode_vector`] for a raw `sqlite-vec` blob.
fn decode_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn setting_get(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let mut stmt = conn
        .prepare("SELECT value FROM settings WHERE key = ?1")
        .map_err(|err| AppError::Database(err.to_string()))?;
    let mut rows = stmt
        .query([key])
        .map_err(|err| AppError::Database(err.to_string()))?;
    match rows
        .next()
        .map_err(|err| AppError::Database(err.to_string()))?
    {
        Some(row) => Ok(Some(
            row.get(0)
                .map_err(|err| AppError::Database(err.to_string()))?,
        )),
        None => Ok(None),
    }
}

fn setting_put(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )
    .map_err(|err| AppError::Database(err.to_string()))?;
    Ok(())
}
