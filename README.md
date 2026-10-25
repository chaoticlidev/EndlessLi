# EndlessLi

EndlessLi is a local-first desktop application for research and knowledge work. It keeps notes, projects, sources, and research sessions in one place; stores them in SQLite on your machine; and sends text to a language model only when you run an AI action. A Python sidecar handles web search, page clipping, PDF text extraction, and local embeddings.

Version 1.0.0 | License: Apache-2.0 | Status: Active

## Features

- **Knowledge base:** Projects, nested folders, tags, full-text search, and a command palette over notes. Notes can be encrypted per project.
- **Research with citations:** Run a light search or a recursive deep dive. Steps stream in, and saved notes keep the source URLs they came from.
- **Search that stays responsive:** Full-text search returned 100 hits from a 10,000-note database in 60 ms in a debug build on the reference Windows machine. The reproduce command and conditions are in [docs/performance.md](./docs/performance.md).
- **Source library:** Import PDFs, Markdown, text, and data files. EndlessLi keeps a managed copy and extracted text, so the original path is not needed again.
- **Providers:** OpenAI-compatible endpoints, Anthropic, and DeepSeek. Keys live in the OS keychain, and token budgets stop a runaway session.
- **Auto-linking:** When the embedding backend is installed, local embeddings suggest related notes after a save. You approve or reject each suggestion; rejections are remembered.
- **Skills and scripts:** Markdown prompt templates with generated parameter forms, plus Python scripts that run in a separate process with a timeout and output caps.
- **Per-project encryption:** AES-256-GCM with a 32-byte key in the OS keychain. Encrypted notes are excluded from search and auto-linking so plaintext never reaches those indexes.
- **Local storage:** SQLite is the source of truth; DuckDB handles corpus analytics. No account is required.

## Quick Start

The first build is the slow part. Cargo compiles DuckDB and the Tauri stack from source, which can take tens of minutes. Later runs reuse the cache.

1. Install the prerequisites in [Requirements](#requirements). On Windows that includes the Visual C++ build tools; on macOS it includes the Xcode command line tools.

2. Clone and install the frontend:

   ```bash
   git clone https://github.com/chaoticlidev/EndlessLi.git endlessli
   cd endlessli
   pnpm install
   ```

3. Create the sidecar environment. On Windows:

   ```powershell
   python -m venv sidecar/.venv
   sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements.txt
   ```

   On macOS or Linux:

   ```bash
   python3 -m venv sidecar/.venv
   sidecar/.venv/bin/python -m pip install -r sidecar/requirements.txt
   ```

   The development launcher looks for `sidecar/.venv` first. Install `sidecar/requirements-embeddings.txt` as well if you want auto-linking.

4. Start the app:

   ```bash
   pnpm tauri dev
   ```

   Vite serves the frontend on port 1420 and a window titled EndlessLi opens.

5. Configure a provider. Open Settings with Ctrl/Cmd+5, choose a provider, paste its API key, and press **Test**. The key goes into the OS keychain and is not shown again.

6. Verify the first run. A short completion appears under the provider form and the token usage summary updates. If Test fails, check the base URL and model before changing anything else.

If a step fails, see [Troubleshooting](./docs/user-guide/troubleshooting.md).

## Requirements

The table lists what a development build needs. The embedding backend is optional; without it, auto-linking is disabled and the rest of the app works.

| Requirement | Minimum | Recommended | Notes |
| --- | --- | --- | --- |
| Windows | Windows 10 or 11 | Same | WebView2 and the Visual C++ build tools with the Desktop development with C++ workload. |
| macOS | 12 | Current release | Install the Xcode command line tools with `xcode-select --install`. |
| Linux | Ubuntu 22.04 or a distribution with WebKitGTK 4.1 | Current Ubuntu LTS | Install `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, and `build-essential`. |
| Node.js | 20 | 22 | CI uses 22. |
| pnpm | 9 | 9 | The repository ships `pnpm-lock.yaml` only; use pnpm. |
| Rust | Stable | Stable | Use the MSVC toolchain on Windows. |
| Python | 3.11 | 3.12 | The sidecar and user scripts share this interpreter. |
| Disk | 10 GB free | 20 GB free | Estimate. Cargo build artifacts dominate; imported files and embedding models add more. |
| Memory | 4 GB | 8 GB | Estimate. Embeddings and large research sessions raise it. |

The only service that needs an account or key is a language-model provider. Research also queries a web search provider; DDGS is the default and works without a key. Local note search, the library, and the database work without either.

## Installation

### Option A: Development Build

The Quick Start covers the development path. To check a checkout before running the app:

```bash
pnpm typecheck
pnpm lint
cargo test --workspace
```

All three commands should exit 0. The Rust suite takes a few minutes on the first run.

The test suite needs the development requirements, which add pytest and the embedding package. Install them once, then run the tests from the repository root. On Windows:

```powershell
sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements-dev.txt
$env:PYTHONPATH = "sidecar"
sidecar\.venv\Scripts\python.exe -m pytest sidecar/tests -q
```

On macOS or Linux:

```bash
sidecar/.venv/bin/python -m pip install -r sidecar/requirements-dev.txt
PYTHONPATH=sidecar sidecar/.venv/bin/python -m pytest sidecar/tests -q
```

### Option B: Packaged Build

Packaged builds need the Python sidecar next to the executable.

1. Build the sidecar with `sidecar/build-sidecar.ps1`. Run it from the repository root on the target operating system. It uses PyInstaller and writes the binary under `src-tauri/binaries/`. The script is PowerShell; on macOS and Linux use PowerShell 7 or run the same PyInstaller command from the script.
2. Tauri expects the sidecar file name to include the target triple, for example `endlessli-sidecar-x86_64-pc-windows-msvc.exe`. Rename the output accordingly. The bundler already points at `binaries/endlessli-sidecar` under `bundle.externalBin` in `src-tauri/tauri.conf.json`.
3. Build the app:

   ```bash
   pnpm tauri build
   ```

4. Find the installer and application bundle under `target/release/bundle/`.

A build without steps 1 and 2 starts, but research and embeddings are unavailable because the sidecar is missing. Scripts then need a Python interpreter on `PATH`.

## Configuration

Normal use needs no configuration file. Provider keys are entered in Settings and stored in the OS keychain; the app does not read a `.env` file at runtime. The repository includes `.env.example` for adapter development, not as a runtime configuration source.

| Setting | Required | Default | Location | Effect |
| --- | --- | --- | --- | --- |
| Provider API key | No | None | Settings, OS keychain | Enables research, skills, and token accounting for that provider. |
| Provider base URL | No | Provider default | Settings | Points an OpenAI-compatible provider at a proxy or local server. |
| Model | No | Provider default | Settings | Model used for research and skills. |
| Embedding backend | No | Off | Settings | Local model that enables auto-linking. Install `sidecar/requirements-embeddings.txt` first. |
| Token budgets | No | No limit | Settings | Daily and monthly limits with warnings and hard stops. |
| `ENDLESSLI_DB_KEY` | No | None | Environment | SQLCipher passphrase for a build with the `sqlcipher` feature. Release builds normally read the keychain. |
| `ENDLESSLI_SIDECAR_DIR` | No | Auto | Environment | Development override for the sidecar directory. |

SQLCipher is a compile-time option, not a setting. The build steps and the OpenSSL requirement are in [docs/encryption.md](./docs/encryption.md).

## Usage

### First Run

1. Open Settings with Ctrl/Cmd+5 and add a provider key. Press **Test** to confirm the key, model, and budget checks.
2. Open the Knowledge Base with Ctrl/Cmd+1. Create a project in the left panel, then create a note. Tags and links live in the note editor.
3. Open Research with Ctrl/Cmd+2. Enter a query, choose **Light search** or **Deep dive**, and wait for the steps to finish. Select results and **Save** them as a note.
4. Open the Library with Ctrl/Cmd+3. Use **Import files** or drag files onto the tab. Open an item to edit metadata, assign tags, or link it to a note.
5. Open Skills with Ctrl/Cmd+4. Fill the parameter form, run a skill, and save the output. Scripts run Python; **Allow network** is off by default and is not an OS sandbox.

### Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl/Cmd+1 | Knowledge Base |
| Ctrl/Cmd+2 | Research |
| Ctrl/Cmd+3 | Library |
| Ctrl/Cmd+4 | Skills |
| Ctrl/Cmd+5 | Settings |
| Ctrl/Cmd+K | Command palette |
| Ctrl/Cmd+E | Toggle note source and preview |

Full workflows are in the [user guide](./docs/user-guide.md): [getting started](./docs/user-guide/getting-started.md), [research](./docs/user-guide/research.md), [memory](./docs/user-guide/memory.md), [linking](./docs/user-guide/linking.md), [skills and scripts](./docs/user-guide/skills-and-scripts.md), and [troubleshooting](./docs/user-guide/troubleshooting.md).

## Documentation

| Document | What it Covers |
| --- | --- |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | System design, data flow, data models, the IPC surface, and the design decisions. |
| [SECURITY.md](./SECURITY.md) | Supported versions, how to report a vulnerability, and the safe harbor policy. |
| [docs/user-guide.md](./docs/user-guide.md) | Orientation: the workspaces, the shortcuts, your data, and what leaves your machine. |
| [docs/user-guide/](./docs/user-guide/) | The task guides, one topic each. |
| [docs/security.md](./docs/security.md) | Key storage, encryption, the script sandbox, and the network policy. |
| [docs/encryption.md](./docs/encryption.md) | Per-project note encryption and the SQLCipher build. |
| [docs/performance.md](./docs/performance.md) | The 10,000-note baseline and how to reproduce it. |
| [docs/README.md](./docs/README.md) | Index of the documentation set. |
| [CHANGELOG.md](./CHANGELOG.md) | What changed, release by release. |

## Project Structure

```text
endlessli/
  src/        React and TypeScript frontend
  src-tauri/  Tauri shell, IPC commands, and window lifecycle
  crates/     Rust workspace: database, LLM, research, skills, security
  sidecar/    Python service for search, clipping, PDF text, and embeddings
  docs/       User, security, encryption, and performance documentation
```

The full crate map and data model are in [ARCHITECTURE.md](./ARCHITECTURE.md).

## Contributing

1. Run the checks before opening a pull request:

   ```bash
   pnpm lint
   pnpm typecheck
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

2. Branch from `main`, keep the change focused, and add a test for new behavior.
3. Update the README section and the relevant document under `docs/` when a behavior changes. Keep the user guide accurate.
4. Run the sidecar tests when you touch `sidecar/`. CI runs the Rust checks on Windows, macOS, and Linux, and the frontend and sidecar checks on Linux.

Changes to the encryption path need the optional SQLCipher test:

```bash
cargo test -p el-db --features sqlcipher --test sqlcipher -- --nocapture
```

That build needs OpenSSL; see [docs/encryption.md](./docs/encryption.md). The default test suite does not enable SQLCipher.

## License

Apache License 2.0. See [LICENSE](./LICENSE).
