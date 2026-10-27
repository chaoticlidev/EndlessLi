# User Guide

This page is the entry point for the user documentation. It covers the workspaces and where your data lives. The task guides linked at the bottom go into detail on one topic each.

## Where to Start

| Guide | Read it when |
| --- | --- |
| [Getting Started](./getting-started.md) | You are installing the app for the first time, or you want a project, a folder, and a note set up. |
| [Research](./research.md) | You want to ask a question with sources and keep the answer. |
| [Synthesize](./synthesize.md) | You want to process a pile of documents into summaries, flashcards, or other media. |
| [Memory](./memory.md) | You want the assistant to remember something, or to stop remembering it. |
| [Linking](./linking.md) | You want related notes to find each other. |
| [Skills and Scripts](./skills-and-scripts.md) | You want to reuse a prompt, or run Python on your material. |
| [Troubleshooting](./troubleshooting.md) | Something did not work. |

## The Five Workspaces

The application has five workspaces accessible from the top navigation bar, plus Settings.

| Workspace | Shortcut | What It Holds |
| --- | --- | --- |
| Knowledge Base | Ctrl/Cmd+1 | Projects, folders, notes, tags, and links |
| Research | Ctrl/Cmd+2 | Search runs, deep dives, sources, and saved notes |
| Library | Ctrl/Cmd+3 | Imported files and saved source material |
| Synthesize | Ctrl/Cmd+4 | Semantic compression, document clustering, and generated outputs |
| Skills | Ctrl/Cmd+5 | Reusable prompts and Python scripts |

**Settings** is a modal window accessed by clicking the gear icon in the top right corner. It holds providers, keys, embedding models, cache, budgets, and memory management.

`Ctrl/Cmd+K` opens the command palette. It navigates, creates notes and projects, searches your notes, and handles the memory slash commands.

`Ctrl/Cmd+E` switches a note between the source editor and the rendered preview.

## Where Your Data Lives

Every user record is in one SQLite file in the platform app data directory, alongside the analytics store, the log file, the managed copy of every imported file, and the cached embedding models. Back up that directory while the app is closed.

Nothing is sent anywhere until you run an AI action. When you do, the text needed for that request goes to the provider you configured in Settings.

## Related Documentation

| Document | What It Covers |
| --- | --- |
| [Security](../security.md) | Key storage, encryption, the script sandbox, and the network policy |
| [Encryption](../encryption.md) | Per-project note encryption and the SQLCipher build |
| [Performance](../performance.md) | The 10,000-note baseline and how to reproduce it |
| [Architecture](../../ARCHITECTURE.md) | System design, data flow, and the repository layout |
| [Security Policy](../../SECURITY.md) | Supported versions and how to report a vulnerability |
