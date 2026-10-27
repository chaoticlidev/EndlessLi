# Security Model

EndlessLi is a single-user, local-first desktop app. There is no server, no account, and no background sync. The security goal is to keep your data on your device, keep provider credentials out of the database, and cap the blast radius of a compromised renderer or a hostile web page.

## API Keys

- Keys live only in the OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service).
- There is no IPC command that returns key material. `api_key_status` reports booleans only; `api_key_test` loads the key into Rust memory for one request.
- The plaintext SQLite fallback that pre-1.0 builds used for keys (`secret.<provider>` settings rows) no longer exists. The keychain is the only key store. On startup the app moves a surviving row into the keychain and then deletes it; if the keychain write fails, the row is left in place so the move can be retried on the next start.
- Keys are never logged. Diagnostics record `provider`, `model_set`, and `key_present` only.

If a key disappears, check the OS credential store before re-adding it. An absent entry means external removal or a keychain backend problem, not an app read path.

## Note Encryption

Per-project AES-256-GCM encryption is available from the lock button on a project. The 32-byte key is generated locally and stored in the OS keychain; only ciphertext (`enc:v1:…`) reaches SQLite. See [Encryption](./encryption.md) for behavior and limitations.

Optional full-database encryption uses SQLCipher and is compiled with the `sqlcipher` Cargo feature. It is off by default and not on the v1.0 critical path.

## Renderer and IPC

- The Tauri capability surface is `core:default` plus a **scoped** opener: only `http://`, `https://`, `mailto:`, and `tel:` URLs can be handed to the system. `reveal_item_in_dir` and unscoped `open_url` are not granted.
- External links click through to the same scoped opener. Invalid or internal links are ignored by the frontend helper as well.
- CSP: `default-src 'self'; script-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; img-src 'self' https: data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost`. Images may come from `https:` because notes render remote images; scripts, objects, form submissions, and base URLs cannot.
- IPC inputs are validated at the command boundary (provider allowlists, setting namespaces, enum values, payload size caps) and again in the repositories. SQL is parameterized.
- LLM output passes one sanitizer (ammonia) and every Markdown render site uses `rehype-sanitize`. Notes are only persisted after an explicit save.

## Web Clipping and Prompt Injection

- The clipper fetches HTTPS only and rejects loopback, link-local, RFC1918, CGNAT, multicast, reserved, `.local`, and `.internal` hosts. Redirects are re-validated hop by hop.
- Clipped source text and search-derived sections are wrapped in explicit delimiters and stripped of control/bidi/zero-width/private-use characters. Prompts state that document content is untrusted data, never instructions.
- Nothing from research is auto-saved: results require user selection and a save action. Model output is sanitized before display.

Residual risk: a hostile page can still bias the summary you read. Treat research output editorially and keep citations in view.

## Script Sandbox

Scripts run in a separate child process under an isolated interpreter (`python -I`), with a fresh temp directory, a wall-clock timeout, output caps, and no inherited secrets. When **Allow network** is off, the runner replaces `socket.socket`, `socket.create_connection`, and `socket.getaddrinfo` with denials.

That network guard is **advisory**, not an OS sandbox. A determined script can bypass it (for example through a subprocess or native APIs) and scripts run with your user permissions. The toggle is off by default. The first time you enable it, the app shows a one-time explanation; after that it does not nag. Only run code you trust, and do not put credentials in scripts.

OS-level confinement (Windows Job objects and AppContainer, macOS sandbox profiles, Linux seccomp and Landlock) is not implemented. Until it is, the honest boundary is: scripts are user-authored code with user-level access.

## Data at Rest

- `endlessli.db` and `logs.db` are SQLite files in the app data directory.
- Logs are diagnostics, not user data, and are pruned on startup (14 days or 5,000 rows, whichever is smaller).
- The `library/` managed file store contains imported copies. Deleting an item removes its copy after canonicalizing the path and checking that it is a regular file inside the store.
- Encrypted project notes are unreadable without their keychain key. Embeddings, link suggestions, and full-text search intentionally skip encrypted notes so plaintext cannot leak through those side channels.

## Reporting

Report suspected vulnerabilities privately rather than opening a public issue with exploit details. The supported versions, the reporting channel, and the safe-harbor terms are in [Security Policy](./SECURITY.md).
