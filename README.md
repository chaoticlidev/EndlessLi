# EndlessLi

**A local-first research and knowledge management tool.**

EndlessLi bridges the gap between your knowledge base and your research workflow. It combines the flexibility of a Notion-style workspace with the power of an AI research assistant. Everything is stored locally in SQLite on your machine, and you only send data to a language model when you explicitly ask it to.

Version 1.0.0 | License: Apache 2.0 | Status: Active Development

## Why EndlessLi?

Most AI tools are cloud-only, lock your data behind a subscription, and require you to switch between your notes and your AI. EndlessLi is different:

* **Local-first:** Your notes, projects, and sources live on your machine.
* **Bring Your Own Key (BYOK):** You aren't paying a middleman. Plug in your OpenAI, Anthropic, DeepSeek, or OpenRouter API key.
* **Deep Research:** Go beyond a simple Google search. EndlessLi can recursively break down complex questions into sub-questions, search the web, clip the sources, and synthesize a cited report.
* **Semantic Compression:** Upload a stack of PDFs, and EndlessLi will cluster them by meaning and generate summaries, slides, or flashcards.

## Features

* **Knowledge Base:** Projects, nested folders, tags, and full-text search. Notes are written in flat Markdown (no clunky block editor).
* **Deep Research:** Run a standard "Light Search" or a recursive "Deep Dive." The UI streams the agent's steps live (Search → Clip → Synthesize) and saves cited sources alongside your notes.
* **Synthesize (Semantic Compression):** Upload PDFs, Markdown, or URLs. Use K-Means clustering to extract the core themes and generate Reports, Flashcards, Quizzes, or Datasets.
* **Search Providers:** A robust fallback chain. Configure multiple providers (DDGS, SearXNG, Tavily, Exa, etc.) with API keys. If one fails, the app moves to the next.
* **Source Library:** Import PDFs and data files. EndlessLi keeps a managed copy and extracts the text, so you can search inside your documents.
* **Auto-Linking:** Local embeddings suggest related notes after you save. You approve or reject each suggestion; rejections are remembered so the AI learns your preferences.
* **Skills & Scripts:** A built-in library of 6 LLM-powered skills (e.g., *Steelman This*, *Action Items*) and 6 local Python scripts (e.g., *YouTube Transcript*, *Basic CSV Cleaner*).
* **Privacy First:** API keys are stored in the OS keychain. You can enable AES-256-GCM encryption for specific projects.

## Quick Start

**Note:** The first build compiles DuckDB and the Tauri stack from source, which can take tens of minutes. Subsequent builds are fast.

### Prerequisites

* **Windows:** Visual Studio C++ Build Tools (Desktop development with C++).
* **macOS:** Xcode Command Line Tools (`xcode-select --install`).
* **Linux:** `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `build-essential`.
* **Tools:** Node.js (v20+), pnpm, Rust (stable), Python (3.11+).

### Installation

1. Clone the repository and install the frontend dependencies:

   ```bash
   git clone https://github.com/chaoticlidev/EndlessLi.git
   cd endlessli
   pnpm install
   ```

2. Set up the Python sidecar (which handles web search, clipping, and embeddings):

   **Windows:**

   ```powershell
   python -m venv sidecar/.venv
   sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements.txt
   ```

   **macOS / Linux:**

   ```bash
   python3 -m venv sidecar/.venv
   sidecar/.venv/bin/python -m pip install -r sidecar/requirements.txt
   ```

3. Start the app:

   ```bash
   pnpm tauri dev
   ```

### First Run

1. Open **Settings** (the gear icon in the top right).
2. Add an API key for your preferred LLM provider (e.g., OpenAI, Anthropic, DeepSeek).
3. Click **Test** to verify the key. It will be saved to your OS keychain and never shown again.
4. Open the **Knowledge Base** to create your first note, or open **Research** to ask a question.

## Documentation

* **[Architecture](./ARCHITECTURE.md):** System design, database schema, and technical decisions.
* **[Security](./SECURITY.md):** How keys are stored, the script sandbox, and vulnerability reporting.
* **User Guide:** *(Coming soon. The current docs are being updated for v1.0).*

## Configuration

Most users will never need to edit a config file. Everything is managed in the Settings UI.

| Setting | Description |
| :--- | :--- |
| **Provider API Keys** | Stored securely in the OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service). |
| **Search Providers** | Choose your primary search engine. DDGS works for free out of the box, but you can add SearXNG, Tavily, or Exa keys for more reliable results. |
| **Token Budgets** | Set a daily or monthly limit to prevent runaway API costs. |
| **Embeddings** | Auto-linking requires a local embedding model. If you don't install it, the app still works, but auto-linking is disabled. |

## Tech Stack

EndlessLi is built with:

* **Frontend:** React, TypeScript, Tailwind CSS, CodeMirror 6.
* **Backend (Desktop):** Tauri 2, Rust.
* **Databases:** SQLite (transactional data), DuckDB (analytical data and corpus chunks).
* **Sidecar (AI & Web):** Python (runs web search, PDF extraction, and local embeddings via ONNX).
* **LLMs:** OpenAI-compatible endpoints, Anthropic, DeepSeek.

## License

This project is licensed under the Apache License 2.0. See [LICENSE](./LICENSE) for details.
