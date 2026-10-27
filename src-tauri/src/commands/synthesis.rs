//! Semantic Compression / Synthesize commands (Phase 14: Synthesize tab).
//!
//! Reuses the chunk -> embed -> cluster -> representative pipeline from
//! `compression.rs`, but instead of emitting one note per cluster it produces a
//! single unified synthesis in the requested media type (report, slides,
//! flashcards, quiz, audio, dataset).
//!
//! `synthesis_start` validates inputs, creates a persisted job, and spawns a
//! background task that streams progress through `synthesis://progress` events.
//! The caller receives the job id immediately and subscribes to events.

use std::sync::Arc;

use el_core::{AppError, AppResult, NewNote};
use el_llm::{ChatMessage, ChatRequest, LlmProvider};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tauri_specta::Event;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// A document to synthesize, mirroring `CompressionDoc`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SynthesisDoc {
    pub doc_ref: String,
    pub text: String,
}

/// The result of a completed synthesis: a title, media-type content, and the
/// doc refs cited inside the content.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SynthesisOutput {
    pub title: String,
    pub content: String,
    pub sources: Vec<String>,
}

/// Progress streamed to the frontend during a synthesis run.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "synthesis://progress")]
pub struct SynthesisProgressEvent {
    pub job_id: String,
    /// Pipeline stage: "in_progress", "chunking", "embedding", "clustering",
    /// "generating", "complete", "error".
    pub stage: String,
    pub message: String,
    #[specta(type = specta_typescript::Number)]
    pub progress: f64,
    /// Present only on the completion event; carries the final output.
    pub output: Option<SynthesisOutput>,
    /// Present only on the error event; carries the failure message.
    pub error: Option<String>,
}

/// Input to `synthesis_start`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SynthesisStartInput {
    pub title: String,
    pub documents: Vec<SynthesisDoc>,
    pub k: Option<u32>,
    #[specta(type = specta_typescript::Number)]
    pub max_context_tokens: Option<u32>,
    pub media_type: String,
    pub instructions: String,
    pub project_id: Option<String>,
    pub folder_id: Option<String>,
}

/// Starts a synthesis run and returns the job id immediately.
///
/// Progress streams through `synthesis://progress` events. The run can be
/// cancelled via `synthesis_cancel`.
#[tauri::command]
#[specta::specta]
pub async fn synthesis_start(
    state: State<'_, AppState>,
    input: SynthesisStartInput,
) -> AppResult<String> {
    if input.documents.is_empty() {
        return Err(AppError::InvalidInput(
            "documents must not be empty".to_string(),
        ));
    }
    if input.documents.len() > 1_000 {
        return Err(AppError::InvalidInput(
            "too many documents (max 1,000)".to_string(),
        ));
    }
    let total_chars: usize = input
        .documents
        .iter()
        .map(|doc| doc.text.chars().count())
        .sum();
    if total_chars > 10_000_000 {
        return Err(AppError::InvalidInput(
            "documents exceed the 10,000,000-character limit".to_string(),
        ));
    }
    if input.k.is_some_and(|value| value == 0) {
        return Err(AppError::InvalidInput("k must be at least 1".to_string()));
    }
    if !["report", "slides", "flashcards", "quiz", "audio", "dataset"]
        .contains(&input.media_type.as_str())
    {
        return Err(AppError::InvalidInput(format!(
            "unsupported media type: {}",
            input.media_type
        )));
    }

    // Persist the job so the history panel can show it immediately.
    let conn = state.db.connect()?;
    let doc_refs_json = serde_json::to_string(
        &input
            .documents
            .iter()
            .map(|doc| doc.doc_ref.clone())
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string());
    let job = el_db::repo::synthesis::create_job(
        &conn,
        &input.title,
        input.k.unwrap_or(1) as i64,
        &input.media_type,
        Some(&input.instructions),
        &doc_refs_json,
    )?;
    drop(conn);

    let job_id = job.id.clone();
    let cancel = CancellationToken::new();
    state.synthesis.insert(job_id.clone(), cancel.clone());

    emit_progress(
        &state,
        &job_id,
        "in_progress",
        "Starting synthesis…".to_string(),
        0.0,
    );

    let task_state = state.inner().clone();
    let task_input = input.clone();
    let task_job_id = job_id.clone();

    tauri::async_runtime::spawn(async move {
        let outcome = run_synthesis(&task_state, &task_job_id, &task_input, cancel).await;
        finalize_job(&task_state, &task_job_id, outcome);
    });

    Ok(job_id)
}

/// Runs the full pipeline inside a background task.
async fn run_synthesis(
    state: &AppState,
    job_id: &str,
    input: &SynthesisStartInput,
    cancel: CancellationToken,
) -> AppResult<SynthesisOutput> {
    let model = crate::embedding::active_model(state)?;
    let embedder_model = model.clone();

    #[cfg(feature = "analytics")]
    let analytics = state
        .analytics
        .as_ref()
        .ok_or_else(|| AppError::Internal("analytics database unavailable".to_string()))?;

    emit_progress(
        state,
        job_id,
        "chunking",
        "Splitting documents into chunks…".to_string(),
        0.05,
    );
    if cancel.is_cancelled() {
        return Err(AppError::Internal("cancelled".to_string()));
    }

    // Chunk
    let docs_json: Vec<serde_json::Value> = input
        .documents
        .iter()
        .map(|doc| {
            // Strip mojibake
            let mut clean_text = doc
                .text
                .replace("â€”", "-")
                .replace("â€“", "-")
                .replace("â€™", "'")
                .replace("â€œ", "\"")
                .replace("â€", "\"");

            // Strip unpaired surrogates
            clean_text = clean_text
                .chars()
                .filter(|c| {
                    let val = *c as u32;
                    !(0xD800..=0xDFFF).contains(&val)
                })
                .collect::<String>();

            serde_json::json!({"doc_ref": doc.doc_ref, "text": clean_text})
        })
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
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if chunks.is_empty() {
        return Err(AppError::InvalidInput("no chunks produced".to_string()));
    }

    let chunk_count = chunks.len();
    let mut chunk_texts: Vec<String> = Vec::with_capacity(chunk_count);
    let mut chunk_doc_refs: Vec<String> = Vec::with_capacity(chunk_count);
    for chunk in &chunks {
        let text = chunk
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim() // Trim whitespace
            .to_string();

        // Skip empty chunks so the embedder doesn't crash
        if text.is_empty() {
            continue;
        }

        chunk_texts.push(text);
        chunk_doc_refs.push(
            chunk
                .get("doc_ref")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
        );
    }

    // Update chunk_count after filtering
    let chunk_count = chunk_texts.len();
    if chunk_count == 0 {
        return Err(AppError::InvalidInput(
            "no readable text chunks produced".to_string(),
        ));
    }

    emit_progress(
        state,
        job_id,
        "embedding",
        format!("Embedding {} chunks…", chunk_count),
        0.20,
    );
    if cancel.is_cancelled() {
        return Err(AppError::Internal("cancelled".to_string()));
    }

    // Embed chunks through cache
    let sidecar = state.sidecar.clone();
    let db = state.db.clone();
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunk_count);
    for (index, text) in chunk_texts.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(AppError::Internal("cancelled".to_string()));
        }
        let (bytes, _) = el_cache::get_or_compute(
            &db,
            el_cache::CacheInput {
                entry_type: "embedding",
                query: text.clone(),
                model: embedder_model.clone(),
                ttl_secs: None,
            },
            || async {
                // Clean the text right before sending it to Python
                let mut cleaned_text = text
                    .replace("â€”", "-")
                    .replace("â€“", "-")
                    .replace("â€™", "'")
                    .replace("â€œ", "\"")
                    .replace("â€", "\"");

                // Strip any remaining non-printable or broken characters
                // This keeps normal letters, numbers, punctuation, and standard spaces
                cleaned_text = cleaned_text
                    .chars()
                    .filter(|c| c.is_ascii() || c.is_alphabetic() || c.is_whitespace())
                    .collect::<String>();

                eprintln!("DEBUG: Sending text to sidecar: {:?}", cleaned_text);

                let result = sidecar
                    .call(
                        "embed.texts",
                        serde_json::json!({"texts": [cleaned_text], "model": embedder_model}), // Use cleaned_text here
                    )
                    .await?;

                eprintln!("DEBUG: Sidecar result: {:?}", result);
                Ok(el_cache::CacheValue {
                    bytes: serde_json::to_vec(&result)
                        .map_err(|err| AppError::Internal(err.to_string()))?,
                    source_type: "embed".to_string(),
                })
            },
        )
        .await?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|err| AppError::Internal(err.to_string()))?;
        let vector = value
            .get("embeddings")
            .and_then(|value| value.as_array())
            .and_then(|array| array.first())
            .and_then(|value| value.as_array())
            .map(|array| {
                array
                    .iter()
                    .map(|value| value.as_f64().unwrap_or(0.0) as f32)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        embeddings.push(vector);
        emit_progress(
            state,
            job_id,
            "embedding",
            format!("Embedded {}/{} chunks…", index + 1, chunk_count),
            0.20 + (0.25 * (index as f64 / chunk_count as f64)),
        );
    }

    // Store chunks in DuckDB
    #[cfg(feature = "analytics")]
    {
        let analytics_job_id = job_id.to_string();
        let mut analytics_chunks = Vec::with_capacity(chunk_count);
        for (index, chunk) in chunks.iter().enumerate() {
            analytics_chunks.push(el_analytics::Chunk {
                id: chunk
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
                job_id: analytics_job_id.clone(),
                doc_ref: chunk_doc_refs[index].clone(),
                chunk_index: chunk
                    .get("chunk_index")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(index as i64),
                text: chunk_texts[index].clone(),
                embedding: Some(embeddings[index].clone()),
            });
        }
        analytics.insert_chunks(&analytics_chunks)?;
    }

    emit_progress(
        state,
        job_id,
        "clustering",
        format!(
            "Clustering {} chunks into {} groups…",
            chunk_count,
            input.k.unwrap_or(8)
        ),
        0.50,
    );
    if cancel.is_cancelled() {
        return Err(AppError::Internal("cancelled".to_string()));
    }

    let k = input
        .k
        .map(|value| (value as usize).clamp(1, chunk_count))
        .unwrap_or(1);

    let emb_json: Vec<Vec<f64>> = embeddings
        .iter()
        .map(|vector| vector.iter().map(|value| *value as f64).collect())
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
        .and_then(|value| value.as_array())
        .map(|array| {
            array
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as usize)
                .collect()
        })
        .unwrap_or_default();
    let centroids: Vec<Vec<f64>> = cluster_resp
        .get("centroids")
        .and_then(|value| value.as_array())
        .map(|array| {
            array
                .iter()
                .map(|row| {
                    row.as_array()
                        .map(|row| {
                            row.iter()
                                .map(|value| value.as_f64().unwrap_or(0.0))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();

    // Representative selection: chunk closest to each centroid
    let mut reps: Vec<Option<usize>> = vec![None; k];
    let mut best_dist: Vec<f64> = vec![f64::INFINITY; k];
    for (index, label) in labels.iter().enumerate() {
        if *label >= k {
            continue;
        }
        let dist = cosine_distance(&embeddings[index], &centroids[*label]);
        if dist < best_dist[*label] {
            best_dist[*label] = dist;
            reps[*label] = Some(index);
        }
    }
    let rep_indices: Vec<usize> = reps.into_iter().flatten().collect();

    emit_progress(
        state,
        job_id,
        "generating",
        format!(
            "Generating {} synthesis from {} themes…",
            input.media_type,
            rep_indices.len()
        ),
        0.65,
    );
    if cancel.is_cancelled() {
        return Err(AppError::Internal("cancelled".to_string()));
    }

    // Gather representative chunk texts with their doc_refs for citations
    // If clustering somehow produced no representatives, fall back to the first
    // chunk of each document so generation still has content to work with.
    let representatives: Vec<(String, String)> = if rep_indices.is_empty() {
        chunk_texts
            .iter()
            .enumerate()
            .map(|(index, text)| (chunk_doc_refs[index].clone(), text.clone()))
            .collect()
    } else {
        rep_indices
            .iter()
            .map(|&index| (chunk_doc_refs[index].clone(), chunk_texts[index].clone()))
            .collect()
    };

    // Generation with media-type-specific prompt
    let provider = Arc::new(crate::commands::provider::build_provider(state)?);
    let llm_model = crate::commands::provider::configured_model(state)?;
    let (content, usage) = generate_synthesis(
        &provider,
        &llm_model,
        &input.media_type,
        &input.instructions,
        &representatives,
        k,
    )
    .await?;

    // Enforce citation: every representative doc_ref should appear.
    let mut content = content;
    for (doc_ref, _) in &representatives {
        if !content.contains(doc_ref) {
            content.push_str(&format!("\n\n[{}]", doc_ref));
        }
    }
    let mut sources: Vec<String> = representatives
        .iter()
        .map(|(doc_ref, _)| doc_ref.clone())
        .collect();
    sources.sort();
    sources.dedup();

    emit_progress(
        state,
        job_id,
        "complete",
        "Synthesis complete.".to_string(),
        1.0,
    );

    let output = SynthesisOutput {
        title: input.title.trim().to_string(),
        content: content.trim().to_string(),
        sources,
    };

    // Persist the completed output and token usage.
    let conn = state.db.connect()?;
    el_db::repo::synthesis::set_output(
        &conn,
        job_id,
        &output.content,
        &llm_model,
        usage.tokens_in,
        usage.tokens_out,
    )?;
    Ok(output)
}

/// Generates the synthesis output using a media-type-specific prompt.
async fn generate_synthesis(
    provider: &Arc<el_llm::AnyProvider>,
    model: &str,
    media_type: &str,
    instructions: &str,
    representatives: &[(String, String)],
    k: usize,
) -> AppResult<(String, el_llm::ChatUsage)> {
    let prompt = build_prompt(media_type, instructions, representatives, k);
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![ChatMessage::user(&prompt)],
        max_tokens: Some(4096),
        temperature: Some(0.3),
    };
    let response = provider.complete(request).await?;
    Ok((response.text, response.usage))
}

/// Builds the LLM prompt for the chosen media type, embedding representative
/// chunk texts with their doc refs for citation.
fn build_prompt(
    media_type: &str,
    instructions: &str,
    representatives: &[(String, String)],
    k: usize,
) -> String {
    let mut chunks_text = String::new();
    for (doc_ref, text) in representatives {
        chunks_text.push_str(&format!("---\nSource [{doc_ref}]:\n{text}\n\n"));
    }

    let instruction = if instructions.trim().is_empty() {
        "No special instructions."
    } else {
        instructions
    };

    match media_type {
        "report" => format!(
            "You are synthesizing a corpus of documents into {k} distinct, comprehensive reports.\n\
             Instructions: {instruction}\n\n\
             Write a comprehensive markdown report that weaves together the themes\n\
             from the following representative chunks. Use clear section headings.\n\
             You MUST cite each source doc_ref at least once in the body using [doc_ref].\n\n\
             {chunks_text}\n\
             Write the {k} reports below, separated by `---`:\n#"
        ),
        "slides" => format!(
            "You are an expert at creating presentations. Convert the following text into a single slide deck.\n\
             Instructions: {instruction}\n\n\
             You MUST output exactly {k} slides. Use the following format:\n\
             ## Slide 1 Title\n- Bullet point 1\n- Bullet point 2\n\n---\n\n\
             ## Slide 2 Title\n- Bullet point 1\n- Bullet point 2\n\n\
             You MUST cite each source doc_ref at least once in the bullet points.\n\n\
             {chunks_text}\n\
             Output the slide deck below:\n"
        ),
        "flashcards" => format!(
            "You are creating a set of flashcards from a corpus of documents.\n\
             Instructions: {instruction}\n\n\
             You MUST output exactly {k} flashcards.\n\
             Output flashcards as `Q: <question>` followed by `A: <answer>` pairs.\n\
             You MUST cite each source doc_ref at least once in the answer.\n\n\
             {chunks_text}\n\
             Write the flashcards below:\n"
        ),
        "quiz" => format!(
            "You are creating a multiple-choice quiz from a corpus of documents.\n\
             Instructions: {instruction}\n\n\
             You MUST output exactly {k} questions.\n\
             Output each question as `Q: <question>` followed by four options \
             (A, B, C, D) and the correct answer on a final line.\n\
             You MUST cite each source doc_ref at least once.\n\n\
             {chunks_text}\n\
             Write the quiz below:\n"
        ),
        "audio" => format!(
            "You are creating {k} distinct spoken-word scripts for text-to-speech from a corpus of documents.\n\
             Instructions: {instruction}\n\n\
             Output clean, spoken-friendly text (no markdown headers needed). Break into paragraphs.\n\
             You MUST cite each source doc_ref at least once, e.g. \"Source: [doc_ref]\".\n\
             Separate the {k} scripts with `---`.\n\n\
             {chunks_text}\n\
             Write the {k} audio scripts below:\n"
        ),
        "dataset" => format!(
            "You are extracting a structured dataset from a corpus of documents.\n\
             Instructions: {instruction}\n\n\
             Output a Markdown table with columns: Field, Value, Source.\n\
             You MUST output exactly {k} rows of data (excluding the header).\n\
             You MUST cite each source doc_ref in the Source column.\n\n\
             {chunks_text}\n\
             Write the dataset table below:\n\n| Field | Value | Source |\n|---|---|---|\n"
        ),
        _ => format!(
            "Synthesize the following content: {chunks_text}"
        ),
    }
}

/// Emits a progress event.
fn emit_progress(state: &AppState, job_id: &str, stage: &str, message: String, progress: f64) {
    let _ = SynthesisProgressEvent {
        job_id: job_id.to_string(),
        stage: stage.to_string(),
        message,
        progress,
        output: None,
        error: None,
    }
    .emit(&state.app_handle);
}

/// Finalizes the job: persists the result or error, emits the terminal event,
/// and removes the run from the cancellation registry.
fn finalize_job(state: &AppState, job_id: &str, outcome: AppResult<SynthesisOutput>) {
    match outcome {
        Ok(output) => {
            let _ = SynthesisProgressEvent {
                job_id: job_id.to_string(),
                stage: "complete".to_string(),
                message: "Synthesis complete.".to_string(),
                progress: 1.0,
                output: Some(output),
                error: None,
            }
            .emit(&state.app_handle);
        }
        Err(err) if is_cancelled(&err) => {
            eprintln!("SYNTHESIS FAILED: {:?}", err);
            let _ = state
                .db
                .connect()
                .and_then(|conn| el_db::repo::synthesis::set_status(&conn, job_id, "cancelled"));
            let _ = SynthesisProgressEvent {
                job_id: job_id.to_string(),
                stage: "error".to_string(),
                message: "Synthesis cancelled.".to_string(),
                progress: 1.0,
                output: None,
                error: Some("cancelled".to_string()),
            }
            .emit(&state.app_handle);
        }
        Err(err) => {
            let _ = state.db.connect().and_then(|conn| {
                el_db::repo::synthesis::set_error(&conn, job_id, &err.to_string())
            });
            let _ = SynthesisProgressEvent {
                job_id: job_id.to_string(),
                stage: "error".to_string(),
                message: "Synthesis failed.".to_string(),
                progress: 1.0,
                output: None,
                error: Some(err.to_string()),
            }
            .emit(&state.app_handle);
        }
    }
    state.synthesis.remove(job_id);
}

/// Returns true when the error is a cooperative cancellation.
fn is_cancelled(err: &AppError) -> bool {
    err.to_string().contains("cancelled")
}

/// Cosine distance between an f32 embedding and an f64 centroid.
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

// --- Read-only commands --------------------------------------------------

/// Returns whether a synthesis run is currently in progress.
#[tauri::command]
#[specta::specta]
pub fn synthesis_status(state: State<'_, AppState>, job_id: String) -> AppResult<SynthesisStatus> {
    let conn = state.db.connect()?;
    let job = el_db::repo::synthesis::get_job(&conn, &job_id)?;
    let running = state.synthesis.contains(&job_id);
    let status = job
        .as_ref()
        .map(|job| job.status.as_str())
        .unwrap_or("not_found");
    Ok(SynthesisStatus {
        job_id,
        status: status.to_string(),
        running,
    })
}

/// Fetches a single synthesis job, if it exists.
#[tauri::command]
#[specta::specta]
pub fn synthesis_get(
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<Option<SynthesisJob>> {
    let conn = state.db.connect()?;
    el_db::repo::synthesis::get_job(&conn, &job_id).map(|job| job.map(Into::into))
}

/// Lists all synthesis jobs newest-first.
#[tauri::command]
#[specta::specta]
pub fn synthesis_jobs(state: State<'_, AppState>, limit: u32) -> AppResult<Vec<SynthesisJob>> {
    let conn = state.db.connect()?;
    el_db::repo::synthesis::list_jobs(&conn, limit as usize)
        .map(|jobs| jobs.into_iter().map(Into::into).collect())
}

/// Cancels a running synthesis. Partial output is kept.
#[tauri::command]
#[specta::specta]
pub fn synthesis_cancel(state: State<'_, AppState>, job_id: String) -> AppResult<()> {
    state.synthesis.cancel(&job_id);

    // Force kill the sidecar to break deadlocks
    if let Ok(mut guard) = state.sidecar.inner.child.lock() {
        if let Some(active_child) = guard.take() {
            let _ = active_child.kill();
        }
    }

    Ok(())
}

/// Deletes a synthesis job. If still running, it is cancelled first.
#[tauri::command]
#[specta::specta]
pub fn synthesis_delete(state: State<'_, AppState>, job_id: String) -> AppResult<()> {
    if state.synthesis.cancel(&job_id) {
        tracing::info!(job_id = %job_id, "cancelled run while deleting job");
    }
    let conn = state.db.connect()?;
    el_db::repo::synthesis::delete_job(&conn, &job_id)
}

/// Saves the synthesis output as a note.
#[tauri::command]
#[specta::specta]
pub fn synthesis_save_output(
    state: State<'_, AppState>,
    job_id: String,
    title: String,
    project_id: Option<String>,
    folder_id: Option<String>,
) -> AppResult<el_core::Note> {
    // Sanitize the project_id. "Inbox" or empty strings mean "No project" (NULL).
    let sanitized_project_id = match project_id.as_deref() {
        Some("Inbox") | Some("") | None => None,
        Some(id) => Some(id.to_string()),
    };

    // Sanitize the folder_id too
    let sanitized_folder_id = match folder_id.as_deref() {
        Some("No folder") | Some("") | None => None,
        Some(id) => Some(id.to_string()),
    };

    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::InvalidInput(
            "title must not be blank".to_string(),
        ));
    }

    let conn = state.db.connect()?;
    let job = el_db::repo::synthesis::require_job(&conn, &job_id)?;
    let output = job.output.as_deref().ok_or_else(|| {
        AppError::InvalidInput("job has no output; wait for completion".to_string())
    })?;

    let mut final_content = output.to_string();

    // Parse the doc_refs string as a JSON array of strings
    if let Ok(source_refs) = serde_json::from_str::<Vec<String>>(&job.doc_refs) {
        for doc_ref in source_refs {
            // Look up the item in the library
            if let Ok(Some(item)) = el_db::repo::library::get(&conn, &doc_ref) {
                // Build a clean markdown link: [Title](URL)
                let url = item.url.unwrap_or_default();
                let replacement = if url.is_empty() {
                    format!("**{}**", item.title)
                } else {
                    format!("[{}]({})", item.title, url)
                };

                // Replace the ID with the Markdown link
                final_content = final_content.replace(&format!("[{}]", doc_ref), &replacement);
                final_content =
                    final_content.replace(&format!("[doc_ref:{}]", doc_ref), &replacement);
            }
        }
    }

    let note = el_db::repo::notes::create(
        &conn,
        &NewNote {
            title: title.clone(),
            content: final_content,
            project_id: sanitized_project_id,
            folder_id: sanitized_folder_id,
            author: "ai".to_string(),
            source_session_id: Some(job_id),
            created_from_note_id: None,
        },
    )?;

    drop(conn);
    state.embed.enqueue(note.id.clone());
    Ok(note)
}

// --- Serializable types for the frontend ---------------------------------

/// Lightweight status returned by `synthesis_status`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SynthesisStatus {
    pub job_id: String,
    pub status: String,
    pub running: bool,
}

/// A persisted synthesis job, mapped from the DB row.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SynthesisJob {
    pub id: String,
    pub title: String,
    pub status: String,
    #[specta(type = specta_typescript::Number)]
    pub k: i64,
    pub media_type: String,
    pub instructions: Option<String>,
    pub doc_refs: String,
    pub model: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub tokens_in: i64,
    #[specta(type = specta_typescript::Number)]
    pub tokens_out: i64,
    pub output: Option<String>,
    pub error_message: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub completed_at: Option<i64>,
}

impl From<el_db::repo::synthesis::SynthesisJob> for SynthesisJob {
    fn from(job: el_db::repo::synthesis::SynthesisJob) -> Self {
        Self {
            id: job.id,
            title: job.title,
            status: job.status,
            k: job.k,
            media_type: job.media_type,
            instructions: job.instructions,
            doc_refs: job.doc_refs,
            model: job.model,
            tokens_in: job.tokens_in,
            tokens_out: job.tokens_out,
            output: job.output,
            error_message: job.error_message,
            created_at: job.created_at,
            completed_at: job.completed_at,
        }
    }
}
