# EndlessLi: Architecture

> Version: 1.0.0
> Status: Active
> Related Docs: [README](./README.md), [SECURITY](./SECURITY.md), [User Guide](./docs/user-guide.md)

## 1. Overview

### 1.1 Purpose

EndlessLi is a desktop application for research and knowledge work. It stores notes, projects, source documents, research sessions, and reusable automation in a local SQLite database, and it talks to a language model only when the user starts an AI action. Everything the user creates stays on the user's machine unless a search provider, a language-model provider, or the user explicitly sends it somewhere else.

The application is split across three runtimes:

- A **React and TypeScript** renderer that draws the six workspaces.
- A **Rust** shell (Tauri) that owns the database, the network calls, the model calls, the cryptography, and the process lifecycle.
- A **Python sidecar** that performs the work Python is genuinely better at: web search, page extraction, PDF text extraction, local embeddings, document chunking, and clustering.

### 1.2 Use Cases

- **Primary use cases:**
  1. Capture notes into projects and folders, tag them, and connect them to related notes automatically.
  2. Ask a question, watch the research steps stream, review the sources, and save the answer as a note that carries its citations.
  3. Import a folder of PDFs and Markdown files into a searchable library and link items to notes.
  4. Use **Semantic Compression** to cluster a corpus of documents and generate reports, slides, flashcards, or quizzes.
  5. Save a prompt as a skill or a Python script and re-run it on new material.
- **Non-goals:** a general-purpose assistant, a cloud sync service, a collaborative editor, a plugin marketplace, and an unrestricted code execution environment. Scripts are bounded automation with a timeout and an output cap, not a REPL.

### 1.3 Core Technologies

| Layer | Technology | Purpose |
| --- | --- | --- |
| Shell | Tauri 2 | Desktop lifecycle, window, IPC, plugin surface |
| Shell language | Rust 2021 edition | Data access, network, crypto, orchestration |
| Renderer | React 18+ | UI |
| Renderer language | TypeScript (strict) | UI, typed IPC bindings |
| Build tooling | Vite | Frontend bundler and dev server |
| Styling | Tailwind CSS | Design tokens and utility styling |
| Server state | TanStack Query | IPC read caching and mutation invalidation |
| UI state | Zustand | Tab, selection, and workspace state |
| Editor | CodeMirror 6 | Markdown and Python source editing |
| Markdown | react-markdown + remark-gfm + rehype-sanitize | Preview rendering under a sanitizer |
| Database | SQLite via rusqlite | Source of truth for user data |
| Vector search | sqlite-vec | Note embeddings in the source-of-truth database |
| Analytics | DuckDB | Corpus chunks for the Synthesize pipeline |
| Crypto | aes-gcm | Per-project note encryption |
| Keychain | keyring | Windows Credential Manager, macOS Keychain, Linux Secret Service |
| HTTP | reqwest with rustls | Model and web requests |
| Sidecar | Python 3.11+ | Search, clipping, PDF, embeddings, chunking, clustering |
| Sidecar transport | JSON-RPC 2.0 over stdio | Shell to sidecar calls |
| IPC typing | specta + tauri-specta | Rust to TypeScript bindings |

### 1.4 Glossary

| Term         | Definition                                                                                    |
| ------------ | --------------------------------------------------------------------------------------------- |
| App shell    | The Rust process that owns the database, the network, and the window.                         |
| Sidecar      | The Python child process that exposes search, clipping, embedding, and clustering over stdio. |
| Renderer     | The webview-hosted React application that draws the UI.                                       |
| Scope        | Where a note sits: Inbox, All, a project, or a folder.                                        |
| Light search | One research pass: search, clip, synthesize.                                                  |
| Deep dive    | A planned research run: sub-questions, child passes, merged synthesis.                        |
| Synthesize   | The Semantic Compression pipeline that clusters documents and generates outputs.              |
| Candidate    | A note proposed as a link, pending user approval.                                             |
| Skill        | A Markdown document with YAML frontmatter that renders into a model prompt.                   |
| Script       | A Python file whose module docstring carries the same YAML plus `allow_network`.              |
| Managed copy | The copy of an imported file that lives inside the app data directory.                        |
| Envelope     | The `enc:v1:<base64(nonceciphertext)>` shape used for encrypted note fields.                  |

## 2. System Architecture

### 2.1 High-Level Component Map

- **Renderer (`src/`):** React application. Owns presentation, local UI state, and the React Query cache over IPC calls. It holds no secrets and never performs a network request of its own.
- **Tauri shell (`src-tauri/`):** Owns the process lifecycle, the SQLite pool, the sidecar child process, the model clients, the keychain, and the IPC command surface.
- **Domain crates (`crates/`):** Eleven Rust libraries with no Tauri dependency, so each one is testable on its own. `el-core` holds the shared domain types and the error type, `el-db` owns persistence, and the rest hold one capability each.
- **Python sidecar (`sidecar/endlessli_sidecar/`):** A stdio JSON-RPC server. The shell owns its lifecycle; the sidecar owns the tools.
- **SQLite (`endlessli.db`):** The source of truth for every user record.
- **DuckDB (`analytics.duckdb`):** An analytical store for corpus chunks produced by the Synthesize pipeline.
- **Logs (`logs.db`):** A pruned SQLite log sink, separate from user data.

### 2.2 Repository Structure

```text
endlessli/
  src/                  React renderer: views, components, IPC bindings, stores
  src-tauri/            Tauri shell: commands, shared state, sidecar manager
  src-tauri/src/commands/  One module per command group
  crates/el-core/       Domain types, vocabularies, the AppError type
  crates/el-db/         Connection pool, migrations, one repo module per entity
  crates/el-llm/        Provider clients, token counting, budget enforcement
  crates/el-cache/      Read-through cache helper over the cache repo
  crates/el-research/   Light and deep orchestration, citation validation
  crates/el-linking/    Embedder trait, embedding pipeline, link suggestions
  crates/el-memory/     Slash command parser and memory context assembly
  crates/el-skills/     Skill and script parsing, rendering, and sandbox
  crates/el-security/   Keychain access, AES-GCM helpers, output sanitizer
  crates/el-analytics/  DuckDB corpus chunk store
  crates/el-compress/   Semantic compression (K-Means) orchestration
  sidecar/              Python package, requirements files, PyInstaller script
  docs/                 User, security, encryption, and performance documentation
  .github/workflows/    CI matrix for frontend, sidecar, Rust, and Tauri builds
```

Each crate directory holds `src/` and optionally `tests/`. `crates/el-db` holds the migration SQL files, and the target schema version is tracked in `migrate.rs`.

### 2.3 Component Interactions

- The **renderer** calls a command through the generated wrapper in `src/ipc/bindings.ts`. The wrapper invokes the snake_case command name (for example `note_list`) and returns a typed result or a typed error.
- The **shell command layer** validates the input at the boundary, then calls a repository function.
- The **shell** calls the **sidecar** through `SidecarManager::call`. The manager starts the child if it is not running, writes one JSON line to the child's stdin, and matches the reply by request id.
- The **shell** calls a **model provider** through `el_llm::run_prompt`, which counts tokens, checks the budget, sends the request, sanitizes the reply, and logs the usage.
- The **embedding worker** waits on the `EmbedQueue`, asks the sidecar for vectors, writes them into the `sqlite-vec` table, and emits `linking://suggestions` when a note gains candidates.
- The **research runner** emits `ResearchEvent` values into a channel. The shell forwards them as the typed `research://step`, `research://result`, and `research://status` events.
- The **synthesis runner** takes uploaded documents, chunks them via the sidecar, stores the chunks in DuckDB, runs K-Means clustering, and calls the LLM to generate the final outputs.

## 3. Data Flow

### 3.1 Primary Flow: Save a Note and Receive Link Suggestions

1. The user types in the editor. The editor debounces and calls `note_update`.
2. The command hands the patch to `el_db::repo::notes::update`. The repository clears the stored embedding model and content hash when the text changed, and re-encrypts the fields when the note lives in an encrypted project.
3. The command enqueues the note id on the embedding queue.
4. The worker wakes, waits 1.5 seconds to coalesce bursts, and calls `el_linking::embed_note` for each queued note.
5. The vector is written to the `note_embeddings` virtual table.
6. `el_linking::suggest_links` queries the nearest neighbours, converts cosine distance to similarity, and returns the candidates above the threshold.
7. The worker emits `linking://suggestions`.
8. The renderer receives the event and lists the candidates. Approving writes a `note_links` row; rejecting writes a `link_rejections` row.

### 3.2 Secondary Flows

**Light research to a saved note.**

1. The user submits a query. `research_start` validates the query and the session memory, resolves the provider and the model, and only then creates the session row.
2. The command registers a cancellation token and spawns `el_research::run_light` in the background.
3. `run_light` records a search step, calls `search.query` on the sidecar, records a clip step, clips the top pages with `clip.url`, and records a synthesis step.
4. The synthesis prompt contains the clipped text wrapped in `<source>` tags. The reply is sanitized, then its citation markers are validated against the source list.
5. The shell persists a `summary` result and one `source` result per source.
6. The user selects results and saves them.

**Deep dive.** `run_deep` asks the model for at most five sub-questions, runs a Light pass for each with a concurrency limit of two, merges the child summaries onto a single deduplicated source list, and synthesizes a merged answer.

**Synthesize (Semantic Compression).**

1. The user uploads files via the Synthesize tab.
2. The Rust shell copies files into the managed library and calls the Python sidecar's `chunk.documents`.
3. The sidecar chunks the text and returns the chunks. The shell embeds them and stores them in DuckDB.
4. The shell calls `cluster.kmeans` on the sidecar to group the embeddings.
5. The shell picks the representative chunk from each cluster and sends it to the LLM to generate the requested output (Report, Flashcards, etc.).
6. The shell streams the progress to the UI and stores the final job in SQLite.

**Script execution.** `script_run` calls `el_skills::run_script`, which writes the source, the parameters, and a Python bootstrap into a private temp directory. The child process runs under `python -I -X utf8 -B`, with a wall-clock timeout, a 1 MB cap per output stream, and the network guard installed unless the run allows network access.

### 3.3 State and Persistence

- **Source of truth for a note:** the `notes` table. Encrypted projects store the title and content as an envelope.
- **Source of truth for embeddings:** the `note_embeddings` virtual table.
- **Source of truth for corpus chunks:** the `corpus_chunks` table in DuckDB.
- **Cache policy:** `cache_entries` with a size ceiling of 1 GB by default. Entries are evicted least-recently-used first.
- **Migrations:** `rusqlite_migration` applies the ordered SQL chain inside `Database::open_with_key`. The version marker is `PRAGMA user_version`. There is no down-migration path.
- **Connection configuration:** every pooled connection is opened with the same pragmas: the SQLCipher key first when present, a 5 second busy timeout, WAL journaling, and foreign keys on.

## 4. Key Design Decisions

### 4.1 Local-First with SQLite as the Source of Truth

- **Chosen approach:** SQLite for every user record, DuckDB for corpus chunks. The two never write to each other.
- **Rationale:** the transactional workload and the analytical workload are genuinely different. SQLite keeps the promise that the database is a single file the user can back up.
- **Trade-offs:** two database engines to maintain. The DuckDB build compiles from source, which makes the first build long.

### 4.2 Rust Orchestrates, Python Provides Tools

- **Chosen approach:** the shell owns orchestration, keys, budgets, and persistence; the sidecar exposes tools only.
- **Rationale:** keys never leave Rust, the model stack (budgets, token logging, sanitizing) is written once and reused by research, compression, and skills, and the sidecar stays replaceable.
- **Trade-offs:** a second process to supervise and a second failure mode.

### 4.3 User-Approved Auto-Linking

- **Chosen approach:** the worker computes candidates and writes nothing. `link_approve` writes one `note_links` row, and `link_reject` records a permanent exclusion.
- **Rationale:** the user stays in control of the graph, and a rejection is respected forever.
- **Trade-offs:** one extra click per suggestion. The threshold is a setting.

### 4.4 User-Controlled Memory

- **Chosen approach:** memories are rows the user creates, with a global and a project scope. Suggested memories wait in `memory_suggestions` until the user accepts one.
- **Rationale:** the user can read exactly what the assistant is told, edit it, and remove it.

### 4.5 Two Tiers for Extensibility: Skills and Scripts

- **Chosen approach:** a skill is a Markdown document that renders into a prompt. A script is a Python file that runs in a separate process.
- **Rationale:** skills are safe for anyone and need no sandbox. Scripts are for users who need computation, and they get a timeout, a temp directory, an output cap, and an opt-in network flag.

### 4.6 sqlite-vec Instead of a Separate Vector Store

- **Chosen approach:** a `vec0` virtual table in the same SQLite file, synchronized with a `notes.embedding_model` tag.
- **Rationale:** one file, one backup, no cross-database consistency rules.
- **Trade-offs:** sqlite-vec is pre-1.0 and its SQL surface can change, so the version is pinned exactly to 0.1.9. The vector table is recreated when the model changes, which means a full re-embed.

### 4.7 Encryption at the Repository Layer

- **Chosen approach:** AES-256-GCM at the note repository layer, with a 32-byte key per project in the OS keychain. SQLCipher remains available as an optional build feature.
- **Rationale:** the user chooses which project holds sensitive material. Because the index tables hold plaintext, encrypted notes are excluded from full-text search, embeddings, auto-linking, and wikilink title resolution.
- **Trade-offs:** encrypted notes are not searchable, and losing the keychain entry loses the notes. There is no recovery key.

## 5. Data Models

All timestamps are Unix epoch seconds stored as integers. Identifiers are UUIDv7 strings.

### 5.1 Note

| Field | Type | Description |
| --- | --- | --- |
| id | TEXT | Primary key |
| title | TEXT | Note title, up to 500 characters |
| content | TEXT | Markdown body, up to 2,000,000 characters |
| project_id | TEXT | Owning project; null means Inbox |
| folder_id | TEXT | Owning folder inside the project |
| author | TEXT | `user`, `ai`, or `import` |
| source_session_id | TEXT | Research session the note came from |
| embedding_model | TEXT | Model that produced the stored vector |
| embedding_hash | TEXT | Hash of the embedded text |

### 5.2 Project

| Field | Type | Description |
| --- | --- | --- |
| id | TEXT | Primary key |
| name | TEXT | Display name, non-blank |
| system_prompt | TEXT | Project-specific model instructions |
| encrypted | INTEGER | 1 when the project's notes are encrypted |

### 5.3 Note Link and Rejection

The pair is stored once with `note_a < note_b`, enforced by a SQL `CHECK` constraint, so a bidirectional link cannot drift into two rows. A rejection is permanent.

### 5.4 Research Session, Step, and Result

Sessions have a `mode` (`light` or `deep`) and a `status` (`pending`, `in_progress`, `completed`, `failed`, `cancelled`). Deleting a session removes its steps and results and detaches the notes it produced.

### 5.5 Memory Entry

A SQL `CHECK` expression ties the type to the presence of a project id, so a global entry can never carry a project and a project entry can never lack one.

### 5.6 Library Item

The type is constrained by a SQL `CHECK`. The extracted content is capped at 1,000,000 characters and the source file at 200 MB. Deleting the item removes the managed copy.

### 5.7 Skill and Script

Both store the whole document so the editor round-trips a file byte for byte, and the derived columns are recomputed at save time.

### 5.8 Synthesis Job

| Field | Type | Description |
| --- | --- | --- |
| id | TEXT | Primary key |
| title | TEXT | User-defined title |
| media_type | TEXT | report, slides, flashcards, quiz, audio, dataset |
| status | TEXT | pending, in_progress, completed, failed |
| k_value | INTEGER | The number of clusters requested |
| created_at | INTEGER | Epoch seconds |

### 5.9 Cache Entry and Token Usage

Search entries expire after 24 hours and clipped pages after 7 days. Model responses and embeddings do not expire.

## 6. API / Interface Reference

### 6.1 Tauri Commands

The shell exposes over 100 commands. Every command returns `AppResult<T>`, which serializes as a typed result or an `AppError` of the shape `{"kind":"...","message":"..."}`.

| Group | Commands |
| --- | --- |
| Projects | `project_list`, `project_create`, `project_update`, `project_archive`, `project_delete`, `project_encryption_set` |
| Folders | `folder_tree`, `folder_create`, `folder_rename`, `folder_move`, `folder_delete` |
| Notes | `note_list`, `note_get`, `note_create`, `note_update`, `note_move`, `note_delete`, `note_branch`, `note_search` |
| Links | `link_create_manual`, `link_remove`, `links_for_note`, `link_reject`, `link_suggestions`, `link_approve` |
| Library | `library_list`, `library_search`, `library_get`, `library_create`, `library_update`, `library_delete`, `library_import_files` |
| Research | `research_start`, `research_cancel`, `research_delete`, `research_get`, `research_history`, `research_save_results` |
| Synthesize | `synthesize_start`, `synthesize_cancel`, `synthesize_get`, `synthesize_list`, `synthesize_save_output` |
| Skills/Scripts | `skill_list`, `skill_save`, `skill_delete`, `skill_run`, `script_list`, `script_save`, `script_delete`, `script_run` |

### 6.2 Sidecar RPC Methods

| Method | Result |
| --- | --- |
| `health.check` | Liveness and version |
| `search.query` | Search results |
| `clip.url` | Extracted web text |
| `embed.texts` | Vectors |
| `pdf.extract` | PDF text |
| `chunk.documents` | Text chunks |
| `cluster.kmeans` | Cluster labels and centroids |

## 7. Dependencies

### 7.1 Required Runtime Dependencies

| Package | Purpose |
| --- | --- |
| tauri | Desktop shell and IPC |
| rusqlite | SQLite access |
| sqlite-vec | Vector similarity |
| reqwest | HTTPS with rustls |
| keyring | OS keychain |
| aes-gcm | Note encryption |
| specta + tauri-specta | Typed IPC bindings |

### 7.2 Optional and Platform-Specific

| Dependency | Behavior When Absent |
| --- | --- |
| `duckdb` (feature `analytics`) | The Synthesize pipeline has nowhere to write. |
| `rusqlite/bundled-sqlcipher` | Supplying a key returns a clear error. |
| `fastembed` (Python extra) | Auto-linking stays off. |
| `pypdf` | PDF imports still create the item, without text. |
| `scikit-learn` | The Synthesize pipeline cannot cluster. |

## 8. Error Handling and Edge Cases

### 8.1 Strategy

Every command returns `AppResult<T>` and every failure carries a `kind` the renderer can branch on. Model and sidecar failures are converted into the same vocabulary, so the renderer does not need a second error path.

### 8.2 Error Catalog

| Scenario | Behavior |
| --- | --- |
| Sidecar crashed | Pending requests fail, the child restarts, status moves to `Restarting`. |
| Sidecar crashed repeatedly | Status becomes `Disabled`. Requires app restart. |
| Model provider returned an error | The body is truncated and returned in the error. |
| Budget exhausted | The request is refused before it is sent. |
| Script exceeded timeout | The child is killed and reported as timed out. |
| Note decryption failed | The read fails with "wrong key or corrupted data". |

### 8.3 Known Edge Cases

- A **session whose run was lost** (the app was killed mid-run) is marked `failed` when it is read.
- **Notes in encrypted projects** are excluded from full-text search, embedding coverage, and wikilink title resolution.
- **The cache ceiling** is compared against total bytes, and the entry that triggered the eviction is protected.
- **Embedding vectors are plaintext.** A note in an encrypted project is never embedded.

## 9. Security Considerations

The full policy lives in [SECURITY](./SECURITY.md). The implementation detail lives in [Security](./docs/security.md). In short:

- **Secrets:** provider keys live only in the OS keychain.
- **Renderer containment:** the capability surface grants a scoped opener.
- **Input handling:** external text is stripped of invisible characters and wrapped in delimiters.
- **Network policy:** the clipper fetches HTTPS only and refuses private addresses.
- **Residual risk:** the script network guard is advisory.

## 10. Performance Considerations

The repeatable baseline is in [Performance](./docs/performance.md). On the reference Windows machine in a debug build, with 10,000 notes: open and migrate 471 ms, list all full rows 99 ms, full-text search returning 100 hits 60 ms.

- **Search is index-backed.** The FTS5 query is wrapped as a phrase.
- **The note list returns full rows.** That is the first candidate for pagination.
- **Embedding is debounced and background.**
- **Repeated research queries are served from the cache.**

## 11. Configuration

There is no configuration file to edit for normal use. Provider keys and every setting live in the OS keychain and in the `settings` table.

| Key | Default | Description |
| --- | --- | --- |
| `llm.provider` | `openai` | Provider id |
| `llm.base_url` | Provider default | Custom endpoint |
| `llm.model` | Provider default | Model used for research |
| `embedding.model` | `sentence-transformers/all-MiniLM-L6-v2` | Embedding model |
| `linking.threshold` | 0.75 | Minimum similarity |
| `budget.daily_tokens` | 0 (unlimited) | Token cap |

## 12. Deployment Architecture

### 12.1 Build

```bash
pnpm install
pnpm lint
pnpm typecheck
pnpm build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
PYTHONPATH=sidecar pytest sidecar/tests -q
```

Regenerate the TypeScript bindings after any change to the command surface:

```bash
cargo test -p endlessli --test export_bindings
```

### 12.2 Run

`pnpm tauri dev` starts the app. A packaged build needs the sidecar binary beside the executable. Run `sidecar/build-sidecar.ps1`, rename the output to include the target triple, and run `pnpm tauri build`.

## 13. Observability

- **Logging.** The `tracing` subscriber writes to the console and to a SQLite sink in `logs.db`.
- **Health check.** `health.check` on the sidecar returns the sidecar version.
- **Token accounting.** Every model call appends to `token_usage`.

## 14. Future Considerations

- **Script interpreter in a packaged build.** Scripts need a Python interpreter.
- **SQLCipher onboarding.** Moving an existing plaintext database to an encrypted one is a manual step.
- **Pagination for the note list.** `note_list` returns full rows for every note in scope.
- **Additional providers.** The provider id flows through `KNOWN_PROVIDERS`, so a new provider is a matter of adding one entry.

## Appendix A. Diagrams

### A.1 Research with a Deep Dive

```text
renderer            shell                  sidecar            provider
   |                  |                       |                   |
   | research_start   |                       |                   |
   |----------------->| resolve key+model     |                   |
   |                  | create session row    |                   |
   |<-----------------| session id            |                   |
   |                  | plan sub-questions    |                   |
   |                  |--------------------------------------->|
   |                  |<---------------------------------------|
   |                  | spawn 2 child Light passes             |
   |                  |  search.query         |                 |
   |                  |---------------------->|                 |
   |                  |<----------------------|                 |
   |                  |  clip.url x5          |                 |
   |                  |---------------------->|                 |
   |                  |<----------------------|                 |
   |                  | synthesize            |                 |
   |                  |--------------------------------------->|
   |                  |<---------------------------------------|
   | research://step   |                       |                   |
   |<-----------------|                       |                   |
   | research://result |                       |                   |
   |<-----------------|                       |                   |
```

### A.2 Note Save to Link Suggestion

```text
renderer          command            repo            worker        sidecar
   |                 |                 |                |              |
   | note_update     |                 |                |              |
   |---------------->| update          |                |              |
   |                 |---------------->|                |              |
   |                 | enqueue(id)     |                |              |
   |                 |------------------------------->|              |
   |                 |                 |  embed_note   |              |
   |                 |                 |--------------->|              |
   |                 |                 |                | embed.texts |
   |                 |                 |                |------------>|
   |                 |                 |                |<------------|
   |                 |                 |  upsert vector |              |
   |                 |                 |<---------------|              |
   |                 |                 |  suggest_links |              |
   |                 |                 |<---------------|              |
   | linking://suggestions              |                |              |
   |<--------------------------------------------------------------|
```

### A.3 Synthesize (Semantic Compression)

```text
renderer         shell              sidecar           duckdb          provider
   |                |                  |                 |                |
   | synthesize_start                  |                 |                |
   |--------------->| copy files       |                 |                |
   |                |----------------->|                 |                |
   |                | chunk.documents  |                 |                |
   |                |----------------->|                 |                |
   |                |<-----------------|                 |                |
   |                | embed chunks     |                 |                |
   |                |----------------->|                 |                |
   |                |<-----------------|                 |                |
   |                | store chunks     |                 |                |
   |                |---------------------------------->|                |
   |                | cluster.kmeans   |                 |                |
   |                |----------------->|                 |                |
   |                |<-----------------|                 |                |
   |                | generate output  |                 |                |
   |                |------------------------------------------------->|
   |                |<-------------------------------------------------|
   | synthesis://progress               |                 |                |
   |<---------------|                  |                 |                |
   | synthesis://done                   |                 |                |
   |<---------------|                  |                 |                |
```

## Appendix B. File Index

| File | Role |
| --- | --- |
| `src-tauri/src/lib.rs` | Application setup: plugins, database, sidecar manager, workers, event forwarders. |
| `src-tauri/src/state.rs` | `AppState`: the database handle, sidecar manager, research registry, embed queue, and paths. |
| `src-tauri/src/sidecar.rs` | Sidecar lifecycle, JSON-RPC transport, restart policy, script interpreter probing. |
| `src-tauri/src/embedding.rs` | The debounced embedding worker and suggestion emission. |
| `src-tauri/src/logging.rs` | SQLite log sink and startup pruning. |
| `src-tauri/src/commands/research.rs` | Research start, cancel, history, and the save-to-note path. |
| `src-tauri/src/commands/synthesis.rs` | Synthesize start, cancel, history, and the save-to-note path. |
| `src-tauri/src/commands/skills.rs` | The skill and script commands. |
| `src-tauri/src/commands/library.rs` | The library commands and the managed file store. |
| `crates/el-db/src/pool.rs` | `Database::open_with_key`: pool, pragmas, migrations, pragma verification. |
| `crates/el-db/src/vec.rs` | The sqlite-vec table, its synchronization, and the similarity query. |
| `crates/el-db/src/repo/notes.rs` | Note encryption, decryption, FTS search, and the embedding-coverage helpers. |
| `crates/el-db/src/repo/cache.rs` | Cache keys, TTLs, and the LRU eviction with a protected latest write. |
| `crates/el-llm/src/provider.rs` | The provider trait and the OpenAI, Anthropic, and DeepSeek clients. |
| `crates/el-llm/src/budgets.rs` | Rolling daily and monthly budget enforcement. |
| `crates/el-research/src/research.rs` | The Light pass, the deep planner, the merge, and cancellation. |
| `crates/el-research/src/citations.rs` | Citation validation and renumbering. |
| `crates/el-linking/src/lib.rs` | The embedding pipeline and the suggestion rules. |
| `crates/el-skills/src/sandbox.rs` | The script sandbox: temp directory, interpreter flags, timeout, output caps, network guard. |
| `crates/el-security/src/secrets.rs` | Keychain access for provider keys, project keys, and the database key. |
| `crates/el-security/src/crypto.rs` | The AES-256-GCM envelope and per-project key management. |
| `sidecar/endlessli_sidecar/server.py` | The JSON-RPC method registry and the stdio loop. |
| `sidecar/endlessli_sidecar/clip.py` | The clipper and its private-address guard. |
| `sidecar/endlessli_sidecar/search.py` | The search provider protocol and the fallback chain. |
| `src/app/App.tsx` | The renderer shell and the tab switch. |
| `src/app/CommandPalette.tsx` | The command palette, note search, and the memory slash commands. |
| `src/ipc/bindings.ts` | Generated TypeScript bindings for every command and event. |
| `src/ipc/queries.ts` | React Query hooks over the IPC surface. |
