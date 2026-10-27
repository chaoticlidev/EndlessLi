# Getting Started

The first run takes a few minutes, mostly because the first build compiles a lot of Rust and, if you want auto-linking, downloads an embedding model. After that the app starts in seconds.

## Prerequisites

### The Operating System Keychain

EndlessLi stores provider API keys in the operating system keychain, never in the database. The keychain has to exist and be unlocked before you can add a key.

| Platform | What it Uses | What You Need |
| --- | --- | --- |
| Windows | Credential Manager | Nothing extra. It is part of the system and unlocks when you sign in. |
| macOS | Keychain | Nothing extra. The first time you store a key, macOS may ask for permission to allow access. |
| Linux | Secret Service | A running Secret Service daemon, usually `gnome-keyring` or `kwallet`. On a desktop session this is normally already there. |

If the keychain is missing, the app still runs. Notes, folders, the library, and search all work. What fails is storing a provider key, and with it every feature that calls a model.

### A Language-Model Account

At least one provider account is needed for research, skills, and the Synthesize pipeline. EndlessLi supports three core providers out of the box:

| Provider | What You Need | Default Model |
| --- | --- | --- |
| OpenAI | An OpenAI API key, or a key for any OpenAI-compatible server | `gpt-4o-mini` |
| Anthropic | An Anthropic API key | `claude-sonnet-4-5` |
| DeepSeek | A DeepSeek API key | `deepseek-chat` |

You can point EndlessLi at any OpenAI-compatible endpoint in Settings, which is how you use a local model server or a proxy (like OpenRouter).

Web search needs no account. The default search backend (DDGS) works without a key. You can add a self-hosted SearXNG instance, Tavily, or a Brave Search key in Settings for more reliable results.

### The Toolchain for a Development Build

| Requirement | Minimum | Recommended |
| --- | --- | --- |
| Node.js | 20 | 22 |
| pnpm | 9 | 9 |
| Rust | Stable | Stable, MSVC toolchain on Windows |
| Python | 3.11 | 3.12 |

Platform packages:

- **Windows:** The Visual C++ build tools with the "Desktop development with C++" workload, and the WebView2 runtime.
- **macOS:** The Xcode command line tools (`xcode-select --install`).
- **Linux:** `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, and `build-essential`.

Allow 10 GB of free disk space for the build cache. The first build of the DuckDB dependency compiles a large C++ library from source and takes a long time. Later builds reuse it.

## Installation

### Development Build

Clone the repository and install the frontend dependencies:

```bash
git clone https://github.com/chaoticlidev/EndlessLi.git endlessli
cd endlessli
pnpm install
```

Create the Python environment for the sidecar. The sidecar is the separate process that performs web search, page extraction, PDF text extraction, and local embeddings.

On Windows:

```powershell
python -m venv sidecar/.venv
sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements.txt
```

On macOS or Linux:

```bash
python3 -m venv sidecar/.venv
sidecar/.venv/bin/python -m pip install -r sidecar/requirements.txt
```

Optionally install the embedding backend, which is what makes auto-linking work:

```powershell
sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements-embeddings.txt
```

Start the app:

```bash
pnpm tauri dev
```

Vite serves the frontend on port 1420 and the application window opens. The sidecar starts lazily, the first time something needs it.

### Packaged Build

A packaged build needs the Python sidecar beside the executable.

1. Build the sidecar on the target operating system:

   ```powershell
   sidecar\build-sidecar.ps1
   ```

   The script uses PyInstaller and writes the binary under `src-tauri/binaries/`. On macOS and Linux, use PowerShell 7 or run the same PyInstaller command from that script.

2. Rename the output so the file name carries the target triple, for example `endlessli-sidecar-x86_64-pc-windows-msvc.exe`. The bundler configuration already points at `binaries/endlessli-sidecar`.

3. Build the application:
   ```bash
   pnpm tauri build
   ```

4. Find the installer under `target/release/bundle/`. You will see two folders:
   * **`nsis/`**: Contains a `.exe` installer. This is the recommended choice. It is smaller, does not require administrative privileges to install, and makes it easy to uninstall later.
   * **`msi/`**: Contains a `.msi` Windows Installer file. Use this if you are deploying EndlessLi across an enterprise network that relies on Group Policy for software distribution.

A build without the sidecar starts and runs, but research, page clipping, PDF extraction, and local embeddings are unavailable. Scripts then need a Python interpreter on your `PATH`.

## Your First Project, Folder, and Note

### 1. Add a Provider Key

Open Settings by clicking the Gear icon in the top right (or press `Ctrl/Cmd+,`).

1. Pick a provider in the Language Model section.
2. Paste the API key into the API Keys section.
3. Click **Test**.

The test sends a one-line completion. When it succeeds you see a short response and a token usage summary under the provider form. The key is in the keychain from this moment, and it is never shown in the interface again. If **Test** fails, check the base URL and the model name first.

### 2. Create a Project

Switch to the Knowledge Base with `Ctrl/Cmd+1`.

The left panel lists Inbox, then your projects. At the top of the panel, click the plus icon, type a name, and confirm. A project is the top level of organization, and it is what makes project-scoped memory and per-project encryption possible.

### 3. Create a Folder

Folders live inside a project and can be nested. Hover a project in the left panel and click the folder icon (**New folder**), then type a name. The tree can be collapsed and expanded with the chevron on each folder.

### 4. Write the First Note

Select the project or folder in the left panel, then click **New note** in the top right. Notes that aren't placed in any project folder default to Inbox.

The note opens in the source editor, because a new note is empty. Type a title and a body. The editor saves on its own a moment after you stop typing, and the status line under the editor shows when the note is saved. Press `Ctrl/Cmd+E` to switch between the source and the rendered preview. Existing notes open in the preview.

### 5. Tag It and Link It

Type in the tag field under the title and press Enter to attach a tag. The links panel at the bottom of the editor lists the notes already linked to this one. You can add a link by searching for a note by title. If you have an embedding backend installed, suggestions appear there too.

### 6. Try a Research Run

Switch to Research with `Ctrl/Cmd+2`, type a question, and choose **Light search**. The steps stream in as the run progresses. When it finishes, select the results you want, press **Save**, give the note a title, pick a project, and save. The saved note carries the summaries and a Sources section with the URLs it came from.

## Where Your Data Lives

Everything the app stores sits in one directory, which the platform picks for you:

| Platform | Location |
| --- | --- |
| Windows | `%APPDATA%\com.endlessli.desktop` |
| macOS | `~/Library/Application Support/com.endlessli.desktop` |
| Linux | `~/.local/share/com.endlessli.desktop` |

Inside it:

* `endlessli.db`: Notes, projects, folders, tags, links, research sessions, memories, library records, skills, scripts, settings.
* `analytics.duckdb`: Corpus chunks produced by the Synthesize pipeline.
* `logs.db`: Diagnostics, pruned to the most recent 14 days and 5,000 rows.
* `library/`: Managed copies of the files you imported.
* `models/`: Downloaded embedding model weights.

Back up this directory while the app is closed. The one thing a file copy cannot capture is a keychain entry, so a backup of an encrypted project is only recoverable if the matching keychain entry is still present.

## Next Steps

| Guide | What it Covers |
| --- | --- |
| [Research](./research.md) | Light search and deep dive, reviewing sources, saving a session as a note. |
| [Synthesize](./synthesize.md) | Uploading documents and generating structured outputs. |
| [Memory](./memory.md) | What the assistant remembers, and how to change it. |
| [Linking](./linking.md) | Automatic link suggestions, approving them, and the threshold. |
| [Skills and Scripts](./skills-and-scripts.md) | Reusable prompts and Python scripts, and how to write each. |
| [Troubleshooting](./troubleshooting.md) | Windows Firewall, what to do when the sidecar disconnects, a key fails, or a script times out. |
