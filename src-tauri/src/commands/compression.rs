//! Semantic Compression commands (Phase 7, Flow 2).

use std::sync::Arc;

use el_core::{AppError, AppResult};
use el_llm::{ChatMessage, ChatRequest, LlmProvider};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CompressionDoc {
    pub doc_ref: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CompressionOutput {
    pub title: String,
    pub content: String,
    pub sources: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn compression_start(
    state: State<'_, AppState>,
    documents: Vec<CompressionDoc>,
    k: Option<u32>,
    max_context_tokens: Option<u32>,
) -> AppResult<Vec<CompressionOutput>> {
    if documents.is_empty() {
        return Err(AppError::InvalidInput(
            "documents must not be empty".to_string(),
        ));
    }
    if documents.len() > 1_000 {
        return Err(AppError::InvalidInput(
            "too many documents (max 1,000)".to_string(),
        ));
    }
    let total_chars: usize = documents.iter().map(|doc| doc.text.chars().count()).sum();
    if total_chars > 10_000_000 {
        return Err(AppError::InvalidInput(
            "documents exceed the 10,000,000-character limit".to_string(),
        ));
    }
    let analytics = state
        .analytics
        .as_ref()
        .ok_or_else(|| AppError::Internal("analytics database unavailable".to_string()))?;
    let model = crate::embedding::active_model(&state)?;
    let embedder_model = model.clone();
    let context_window = max_context_tokens.unwrap_or(8192) as usize;

    // 1. chunk
    let docs_json: Vec<serde_json::Value> = documents
        .iter()
        .map(|d| serde_json::json!({"doc_ref": d.doc_ref, "text": d.text}))
        .collect();
    let resp = state
        .sidecar
        .call(
            "chunk.documents",
            serde_json::json!({"documents": docs_json, "chunk_size": 800, "overlap": 100}),
        )
        .await?;
    let chunks = resp
        .get("chunks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if chunks.is_empty() {
        return Err(AppError::InvalidInput("no chunks produced".to_string()));
    }

    // 2. embed chunks through cache
    let mut chunk_texts: Vec<String> = Vec::with_capacity(chunks.len());
    let mut chunk_doc_refs: Vec<String> = Vec::with_capacity(chunks.len());
    let mut chunk_ids: Vec<String> = Vec::with_capacity(chunks.len());
    for c in &chunks {
        chunk_ids.push(
            c.get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
        chunk_doc_refs.push(
            c.get("doc_ref")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
        chunk_texts.push(
            c.get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
    }

    let sidecar = state.sidecar.clone();
    let db = state.db.clone();
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    for text in &chunk_texts {
        let (bytes, _) = el_cache::get_or_compute(
            &db,
            el_cache::CacheInput {
                entry_type: "embedding",
                query: text.clone(),
                model: embedder_model.clone(),
                ttl_secs: None,
            },
            || async {
                let r = sidecar
                    .call(
                        "embed.texts",
                        serde_json::json!({"texts": [text], "model": embedder_model}),
                    )
                    .await?;
                Ok(el_cache::CacheValue {
                    bytes: serde_json::to_vec(&r).map_err(|e| AppError::Internal(e.to_string()))?,
                    source_type: "embed".to_string(),
                })
            },
        )
        .await?;
        let r: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| AppError::Internal(e.to_string()))?;
        let vec = r
            .get("embeddings")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .map(|x| x.as_f64().unwrap_or(0.0) as f32)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        embeddings.push(vec);
    }

    // 3. store in duckdb
    let job_id = uuid::Uuid::now_v7().to_string();
    let mut analytics_chunks = Vec::with_capacity(chunks.len());
    for (i, c) in chunks.iter().enumerate() {
        analytics_chunks.push(el_analytics::Chunk {
            id: chunk_ids[i].clone(),
            job_id: job_id.clone(),
            doc_ref: chunk_doc_refs[i].clone(),
            chunk_index: c
                .get("chunk_index")
                .and_then(|v| v.as_i64())
                .unwrap_or(i as i64),
            text: chunk_texts[i].clone(),
            embedding: Some(embeddings[i].clone()),
        });
    }
    analytics.insert_chunks(&analytics_chunks)?;

    // 4. K fallback
    let chunk_count = chunks.len();
    let avg_tokens: usize = chunk_texts
        .iter()
        .map(|t| t.len() / 4)
        .sum::<usize>()
        .checked_div(chunk_count)
        .map_or(1, |tokens| tokens.max(1));
    let auto_k = ((context_window as f64 * 0.6) / avg_tokens as f64).floor() as usize;
    let auto_k = auto_k.clamp(1, chunk_count);
    let k = k
        .map(|v| (v as usize).clamp(1, chunk_count))
        .unwrap_or(auto_k);

    // 5. cluster
    let emb_json: Vec<Vec<f64>> = embeddings
        .iter()
        .map(|v| v.iter().map(|x| *x as f64).collect())
        .collect();
    let cluster_resp = state
        .sidecar
        .call(
            "cluster.kmeans",
            serde_json::json!({"embeddings": emb_json, "k": k, "seed": el_compress::CLUSTER_SEED}),
        )
        .await?;
    let labels: Vec<usize> = cluster_resp
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0) as usize).collect())
        .unwrap_or_default();
    let centroids: Vec<Vec<f64>> = cluster_resp
        .get("centroids")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|row| {
                    row.as_array()
                        .map(|r| r.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();

    // 6. representative selection: chunk closest to each centroid
    let mut reps: Vec<Option<usize>> = vec![None; k];
    let mut best_dist: Vec<f64> = vec![f64::INFINITY; k];
    for (idx, label) in labels.iter().enumerate() {
        if *label >= k {
            continue;
        }
        let dist = cosine_distance(&embeddings[idx], &centroids[*label]);
        if dist < best_dist[*label] {
            best_dist[*label] = dist;
            reps[*label] = Some(idx);
        }
    }
    let mut rep_indices: Vec<usize> = reps.into_iter().flatten().collect();
    if rep_indices.is_empty() {
        rep_indices.push(0);
    }

    // 7. generation
    let provider = Arc::new(crate::commands::provider::build_provider(&state)?);
    let llm_model = crate::commands::provider::configured_model(&state)?;
    let mut outputs = Vec::new();
    for (i, &idx) in rep_indices.iter().enumerate() {
        let doc_ref = &chunk_doc_refs[idx];
        let chunk_text = &chunk_texts[idx];
        let prompt = format!(
            "You are compressing a document corpus into a structured note.\n\
             Write a concise markdown note (title on first line as '# Title', then body) \
             capturing the theme of the following representative chunk.\n\
             You MUST cite the source document ref [{doc_ref}] at least once in the body.\n\n\
             Source doc_ref: {doc_ref}\n\
             Chunk:\n{chunk_text}"
        );
        let request = ChatRequest {
            model: llm_model.clone(),
            messages: vec![ChatMessage::user(&prompt)],
            max_tokens: Some(512),
            temperature: Some(0.3),
        };
        let response = provider.complete(request).await?;
        let mut content = response.text.trim().to_string();
        if !content.contains(doc_ref) {
            content.push_str(&format!("\n\nSource: [{}]", doc_ref));
        }
        let title = content
            .lines()
            .next()
            .unwrap_or("")
            .trim_start_matches('#')
            .trim()
            .to_string();
        let title = if title.is_empty() {
            format!("Compressed note {}", i + 1)
        } else {
            title
        };
        outputs.push(CompressionOutput {
            title,
            content,
            sources: vec![doc_ref.clone()],
        });
    }

    Ok(outputs)
}

#[tauri::command]
#[specta::specta]
pub fn compression_save_outputs(
    state: State<'_, AppState>,
    outputs: Vec<CompressionOutput>,
    project_id: Option<String>,
    folder_id: Option<String>,
) -> AppResult<Vec<el_core::Note>> {
    if outputs.is_empty() {
        return Err(AppError::InvalidInput(
            "outputs must not be empty".to_string(),
        ));
    }
    let conn = state.db.connect()?;
    let mut notes = Vec::new();
    for out in outputs {
        let note = el_db::repo::notes::create(
            &conn,
            &el_core::NewNote {
                title: out.title,
                content: out.content,
                project_id: project_id.clone(),
                folder_id: folder_id.clone(),
                author: "ai".to_string(),
                source_session_id: None,
                created_from_note_id: None,
            },
        )?;
        notes.push(note);
    }
    drop(conn);
    for note in &notes {
        state.embed.enqueue(note.id.clone());
    }
    Ok(notes)
}

fn cosine_distance(a: &[f32], b: &[f64]) -> f64 {
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    let n = a.len().min(b.len());
    for i in 0..n {
        let x = a[i] as f64;
        let y = b[i];
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    1.0 - (dot / denom)
}
