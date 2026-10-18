//! DuckDB engine behind the `duckdb` feature: the `corpus_chunks` store.
//!
//! `Analytics` owns one `Connection` behind a `Mutex` (DuckDB connections are
//! not `Sync`). It is opened at app start and shared through `AppState`; the
//! Semantic Compression pipeline (Phase 7) inserts chunks and reads them back
//! for clustering.

use std::path::Path;
use std::sync::Mutex;

use el_core::{AppError, AppResult};

use crate::CORPUS_CHUNKS_TABLE;

/// One row of `corpus_chunks`: a chunk of a source document plus its optional
/// embedding. `job_id` groups the chunks of a single compression job.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub id: String,
    pub job_id: String,
    pub doc_ref: String,
    pub chunk_index: i64,
    pub text: String,
    pub embedding: Option<Vec<f32>>,
}

/// The analytical database: a single DuckDB file plus its `corpus_chunks`
/// table. Shared as `Arc<Analytics>` at the call site; internally
/// synchronized.
pub struct Analytics {
    conn: Mutex<duckdb::Connection>,
}

impl std::fmt::Debug for Analytics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analytics").finish_non_exhaustive()
    }
}

fn map_duck(err: duckdb::Error) -> AppError {
    AppError::Database(format!("duckdb: {err}"))
}

impl Analytics {
    /// Opens (creating parent directories) the analytical file and ensures
    /// the `corpus_chunks` schema exists.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| AppError::Database(format!("create analytics dir: {err}")))?;
            }
        }
        let conn = duckdb::Connection::open(path).map_err(map_duck)?;
        let analytics = Self {
            conn: Mutex::new(conn),
        };
        analytics.ensure_schema()?;
        tracing::info!(path = %path.display(), "analytics database opened");
        Ok(analytics)
    }

    /// Creates the `corpus_chunks` table when it does not exist. Idempotent.
    pub fn ensure_schema(&self) -> AppResult<()> {
        let conn = self.conn.lock().expect("analytics lock");
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {CORPUS_CHUNKS_TABLE} (
               id TEXT PRIMARY KEY,
               job_id TEXT NOT NULL,
               doc_ref TEXT NOT NULL,
               chunk_index BIGINT NOT NULL,
               text TEXT NOT NULL,
               embedding FLOAT[]
             );"
        ))
        .map_err(map_duck)?;
        Ok(())
    }

    /// Inserts or replaces chunks. The embedding is bound as a DuckDB list
    /// literal cast to `FLOAT[]` so no list type mapping is required.
    pub fn insert_chunks(&self, chunks: &[Chunk]) -> AppResult<usize> {
        if chunks.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().expect("analytics lock");
        let sql = format!(
            "INSERT OR REPLACE INTO {CORPUS_CHUNKS_TABLE}
               (id, job_id, doc_ref, chunk_index, text, embedding)
             VALUES (?, ?, ?, ?, ?, CAST(? AS FLOAT[]))"
        );
        let mut stmt = conn.prepare(&sql).map_err(map_duck)?;
        for chunk in chunks {
            let embedding = chunk
                .embedding
                .as_ref()
                .map(|vector| format_float_list(vector));
            stmt.execute(duckdb::params![
                chunk.id,
                chunk.job_id,
                chunk.doc_ref,
                chunk.chunk_index,
                chunk.text,
                embedding
            ])
            .map_err(map_duck)?;
        }
        Ok(chunks.len())
    }

    /// Total number of stored chunks.
    pub fn count_chunks(&self) -> AppResult<u64> {
        let conn = self.conn.lock().expect("analytics lock");
        let count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {CORPUS_CHUNKS_TABLE}"),
                [],
                |row| row.get(0),
            )
            .map_err(map_duck)?;
        Ok(count.max(0) as u64)
    }

    /// Returns the `(id, text)` of every chunk in a job, ordered by source
    /// document then chunk index. Proves the store is queryable from Rust.
    pub fn chunk_texts(&self, job_id: &str) -> AppResult<Vec<(String, String)>> {
        let conn = self.conn.lock().expect("analytics lock");
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, text FROM {CORPUS_CHUNKS_TABLE}
                 WHERE job_id = ? ORDER BY doc_ref, chunk_index"
            ))
            .map_err(map_duck)?;
        let rows = stmt
            .query_map([job_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(map_duck)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_duck)?);
        }
        Ok(out)
    }
}

/// Formats a vector as a DuckDB list literal, e.g. `[0.1, -0.2]`.
fn format_float_list(vector: &[f32]) -> String {
    let mut out = String::with_capacity(vector.len() * 12 + 2);
    out.push('[');
    for (index, value) in vector.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

#[cfg(all(test, feature = "duckdb"))]
mod tests {
    use super::*;

    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "endlessli-analytics-{}.duckdb",
            uuid::Uuid::now_v7()
        ))
    }

    #[test]
    fn schema_insert_and_query() {
        let path = temp_path();
        let analytics = Analytics::open(&path).expect("open analytics");
        assert_eq!(analytics.count_chunks().expect("count"), 0);

        let chunks = vec![
            Chunk {
                id: "c1".to_string(),
                job_id: "job1".to_string(),
                doc_ref: "docA".to_string(),
                chunk_index: 0,
                text: "first chunk".to_string(),
                embedding: Some(vec![0.1, 0.2, 0.3]),
            },
            Chunk {
                id: "c2".to_string(),
                job_id: "job1".to_string(),
                doc_ref: "docA".to_string(),
                chunk_index: 1,
                text: "second chunk".to_string(),
                embedding: None,
            },
        ];
        assert_eq!(analytics.insert_chunks(&chunks).expect("insert"), 2);
        assert_eq!(analytics.count_chunks().expect("count"), 2);

        // Re-inserting replaces rather than duplicates.
        analytics.insert_chunks(&chunks[..1]).expect("replace");
        assert_eq!(analytics.count_chunks().expect("count"), 2);

        let texts = analytics.chunk_texts("job1").expect("query");
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].1, "first chunk");
        assert_eq!(texts[1].1, "second chunk");

        drop(analytics);
        std::fs::remove_file(&path).ok();
    }
}
