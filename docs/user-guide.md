# User Guide

This page is the orientation guide. The task guides linked at the bottom go into detail on one topic each.

## The Five Workspaces

The application is one window with five main workspaces accessible from the top navigation bar, plus a settings modal.

### Knowledge Base (Ctrl/Cmd+1)

Where your writing lives. The left panel lists Inbox, your Projects, and the Folders inside each Project. You can create, rename, archive, and delete Projects and Folders from that panel.

The main area lists the Notes in the selected scope. Search by text, filter by tag, and switch between a bento grid and a list. A Note opens in the rendered preview; a new or empty Note opens in the source editor. Use Ctrl/Cmd+E or the Source and Preview buttons to switch.

Each Note has a tag field and a links panel. A Project can be encrypted, which encrypts every Note in it with a key held in the operating system Keychain. Encrypted Notes are not searchable and do not take part in auto-linking, on purpose.

### Research (Ctrl/Cmd+2)

Ask a question and get an answer with sources. A Light Search is one pass: search, read the pages, synthesize. A Deep Dive plans sub-questions, runs several passes, and merges the findings.

Steps stream in while the run happens, and Stop cancels a run in progress while keeping what already finished. Select the results you want and save them as a Note; the Note carries a Sources section with the URLs it came from. The History panel reopens past sessions.

### Library (Ctrl/Cmd+3)

Source material that outlives a single research session. Import PDFs, Markdown, text, and data files with the button or by dragging them onto the tab. The file is copied into the app data directory, and the original path is never needed again. Search by title and content, filter by type, tag, or source, and link an item to a Note or create a Note from it.

### Synthesize (Ctrl/Cmd+4)

Semantic compression. Upload multiple documents, choose how many outputs you want (K-Means clustering), and generate Reports, Flashcards, Quizzes, or Datasets. The AI extracts the core, non-overlapping themes from your documents and synthesizes them into structured outputs.

### Skills and Scripts (Ctrl/Cmd+5)

Two kinds of reusable automation. A Skill is a Markdown prompt with YAML frontmatter; you fill in its parameters and run it. A Script is a Python file whose docstring holds the same metadata; it runs in a separate process with a timeout and an output cap. Both can save their output as a Note.

### Settings (Gear Icon)

It is accessed by clicking the gear icon in the top right, or by pressing `Ctrl/Cmd+,`.

Here you configure Providers and API keys, the embedding model and its coverage, the cache, token budgets, the memory manager, and check the analytics status.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl/Cmd+1 | Knowledge Base |
| Ctrl/Cmd+2 | Research |
| Ctrl/Cmd+3 | Library |
| Ctrl/Cmd+4 | Synthesize |
| Ctrl/Cmd+5 | Skills |
| Ctrl/Cmd+K | Command Palette |
| Ctrl/Cmd+E | Toggle Note source and preview |

The Command Palette navigates, creates Notes and Projects, searches your Notes, and handles the Memory slash commands (`/remember`, `/project remember`, `/forget`, `/showmem`).

## Your Data

User data lives in the platform app-data directory, in one SQLite file called `endlessli.db`, next to `analytics.duckdb`, `logs.db`, a `library/` folder holding the managed copies of imported files, and a `models/` folder holding cached embedding weights.

| Platform | Location |
| --- | --- |
| Windows | `%APPDATA%\com.endlessli.desktop` |
| macOS | `~/Library/Application Support/com.endlessli.desktop` |
| Linux | `~/.local/share/com.endlessli.desktop` |

Back up that directory while the app is closed. Logs are diagnostics rather than user data, and they are pruned on startup to the most recent 14 days and 5,000 rows.

## What Leaves Your Machine

Nothing, until you run an AI action.

| Action | What is Sent | Where it Goes |
| --- | --- | --- |
| Research | The query, and the text of the pages that were read. | Your model provider, and the search provider for the query. |
| A Skill | The rendered prompt. | Your model provider. |
| A Script | Nothing, unless it opens a connection itself. | Wherever the script connects to. |
| Auto-Linking | Nothing. The embedding runs locally. | Nowhere. |

The web clipper refuses to fetch anything that is not a public HTTPS address, so it will not reach into your local network. Provider keys live in the operating system Keychain and are never written to the database.

The details are in [security.md](./security.md).

## Task Guides

| Guide | Covers |
| --- | --- |
| [Getting Started](./user-guide/getting-started.md) | Prerequisites, installation, and your first Project, Folder, and Note. |
| [Research](./user-guide/research.md) | Light Search and Deep Dive, sources, and saving a session as a Note. |
| [Synthesize](./user-guide/synthesize.md) | Uploading documents and generating structured outputs. |
| [Memory](./user-guide/memory.md) | What the assistant remembers, the slash commands, and the Memory manager in Settings. |
| [Linking](./user-guide/linking.md) | Link suggestions, approving them, the threshold, and manual links. |
| [Skills and Scripts](./user-guide/skills-and-scripts.md) | Writing a Skill, writing a Script, and the network sandbox. |
| [Troubleshooting](./user-guide/troubleshooting.md) | The sidecar disconnecting, a key that fails, and a script that times out. |
