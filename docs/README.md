# EndlessLi Documentation

This directory contains the user-facing documentation for EndlessLi.

The repository root holds the documents that describe the project itself: the [README](../README.md) and the [Architecture](../ARCHITECTURE.md).

## The Documentation Set

| Document | What it Covers | Read it When |
| --- | --- | --- |
| [User Guide](./user-guide.md) | Orientation: the workspaces, the shortcuts, where your data lives, and what leaves your machine. | You are new to the application. |
| [User Guide Folder](./user-guide/) | The task guides, one topic each. | You want to do something specific. |
| [Security](./security.md) | Key storage, note encryption, the script sandbox, the network policy, and the renderer boundary. | You want to know how the app protects your data. |
| [Encryption](./encryption.md) | Per-project note encryption, what it excludes, and the optional SQLCipher build. | You are encrypting a project or building with database encryption. |
| [Performance](./performance.md) | The 10,000-note baseline, the numbers, and how to reproduce them. | You want to know how fast it is or check it on your machine. |
| [Architecture](../ARCHITECTURE.md) | System design, data flow, data models, the IPC surface, and the design decisions. | You are reading the code, extending it, or reviewing it. |
| [Security Policy](../SECURITY.md) | Supported versions, how to report a vulnerability, and safe harbor. | You found a security problem. |

## The Task Guides

| Guide | Covers |
| --- | --- |
| [Getting Started](./user-guide/getting-started.md) | Prerequisites, installation, and creating your first project, folder, and note. |
| [Research](./user-guide/research.md) | Light search and deep dive, reviewing sources, saving a session as a note, history, and cost control. |
| [Synthesize](./user-guide/synthesize.md) | Uploading documents, using K-Means clustering, and generating reports, slides, or flashcards. |
| [Memory](./user-guide/memory.md) | Global and project memory, the slash commands in the command palette, and managing memory in Settings. |
| [Linking](./user-guide/linking.md) | How link suggestions work, approving and rejecting them, the threshold, and manual links. |
| [Skills and Scripts](./user-guide/skills-and-scripts.md) | The difference between a skill and a script, how to write each, and the network sandbox. |
| [Troubleshooting](./user-guide/troubleshooting.md) | The sidecar disconnecting, an API key that fails, a script that times out, and smaller problems. |

## Conventions Used Here

- **The application is called EndlessLi.** The workspace is a Project, and a saved piece of writing is a Note.
- **Keychain** means the credential store of the operating system: Windows Credential Manager, macOS Keychain, and the Linux Secret Service.
- **The Sidecar** is the separate Python process that performs web search, page clipping, PDF text extraction, and local embeddings.
- **Ctrl** refers to the Command key on macOS. Both platforms use the same shortcuts.
- Code blocks show commands. A `powershell` block is for Windows, a `bash` block is for macOS and Linux.
