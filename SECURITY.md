# Security Policy

> Project: EndlessLi
> Last reviewed: 2026-10-20
> Applies to: 1.x
> Reporting channel: see [Reporting a Vulnerability](#2-reporting-a-vulnerability)

## 1. Supported Versions

EndlessLi is a single-user desktop application with no server component and
no background sync. Security fixes land on the current release line.

| Version | Supported | Notes |
| --- | --- | --- |
| 1.x | Yes | Receives fixes. The current release is 1.0.0. |
| Anything older than 1.0 | No | Upgrade to a supported release. There is no migration path from a pre-1.0 build. |

Fixes land on the latest minor of the supported line. Older minors on that
line must upgrade to receive them. There is no backport to a specific patch
version, and no end-of-life date has been set for the 1.x line.

Because EndlessLi stores data locally and talks to third-party providers on
the user's behalf, the most valuable fixes are the ones that reach a user
quickly. If a critical issue is found, the maintainer's intent is to publish
a patch release promptly rather than wait for a feature release.

## 2. Reporting a Vulnerability

Please do not open a public issue for a suspected vulnerability. Public
issues are visible immediately and give an attacker a head start.

### 2.1 How to Report

1. Use GitHub's private vulnerability reporting on the repository:
   **Security** tab, then **Report a vulnerability**. This opens a private
   channel with the maintainer and the repository's security team. If the
   tab is not enabled on the repository, open a regular issue that contains
   only a one-line notice that a private report will follow, and no details.
2. Write the subject as `[SECURITY] brief description`.
3. Include:
   - The affected version, and how it was installed (packaged build or a
     development checkout).
   - The operating system and its version.
   - What an attacker could achieve, in general terms.
   - Steps to reproduce, at a high level. Weaponized proof-of-concept code is
     not needed in the first message.
   - Any error output or logs, with keys and personal data redacted.
4. Do not include live credentials, personal data, or exploit automation in
   the first message. The maintainer will ask for anything further over the
   private channel.

Do not test against data that is not yours. Use a test account and a scratch
database.

### 2.2 What Happens Next

- **Acknowledgement:** within 3 business days.
- **Triage:** within 7 business days, with a severity assessment and an
  indication of whether the issue is accepted.
- **Fix and disclosure:** an accepted issue is fixed on the supported line.
  The fix is recorded in the CHANGELOG under Security, and the reporter is
  credited in the release notes unless anonymity is requested.
- **Scope:** issues that affect supported versions in the default
  configuration are prioritized. An issue that requires a custom build, a
  modified provider configuration, or running unreviewed user scripts is
  considered for mitigation advice rather than a patch.

### 2.3 Safe Harbor

Good-faith research is welcome.

- Do not access data that is not yours, do not read other users' notes, and
  do not exfiltrate data beyond what is needed to demonstrate the issue.
- Do not degrade the service, degrade the local database, or leave the
  application or its data in a broken state.
- Do not create accounts, send spam, or use third-party services to attack
  the project.
- Do not disclose the issue publicly before the fix window agreed during
  triage.
- Give the maintainer a reasonable amount of time to release a fix before
  any disclosure.

Researchers who act within this scope will not face adverse action from the
project or its maintainer, and will be credited if they wish to be.

## 3. Security Practices

The implementation of these practices is described in
[docs/security.md](./docs/security.md). This section states what the project
guarantees and where each guarantee is enforced.

### 3.1 Authentication and Authorization

EndlessLi has no accounts, no server, and no authentication surface. The
user owns the device and the data, and there is no remote party to
authenticate. The only credentials in the system are third-party API keys that
the user supplies and stores in the operating system keychain.

### 3.2 Data Protection

- Provider API keys are stored only in the operating system keychain, under
  the service name `endlessli`. The keychain is the sole key store; there is
  no fallback that keeps key material in the database file.
  (`crates/el-security/src/secrets.rs:10`)
- No IPC command returns key material. Status commands report whether a key
  is present. (`src-tauri/src/commands/secrets.rs:121`)
- Notes can be encrypted per project with AES-256-GCM, using a key held in
  the operating system keychain. (`crates/el-security/src/crypto.rs:66`)
- Full-database encryption is available as an optional build feature using
  SQLCipher. It is off by default. (`crates/el-db/src/pool.rs:73`)
- Transport to model providers and search providers uses HTTPS with rustls.
  (`crates/el-llm/src/provider.rs:92`)
- Encrypted notes are deliberately excluded from full-text search,
  embeddings, auto-linking, and wikilink resolution, so plaintext cannot
  leak through those indexes. (`crates/el-linking/src/lib.rs:131`)

### 3.3 Input Handling and Injection Resistance

- All SQL is parameterized. The only dynamic SQL in the project builds the
  column list, not the values, and every value is bound.
  (`crates/el-db/src/repo/library.rs:199`)
- Full-text search input is wrapped as a phrase, so a user-supplied string
  cannot produce a query syntax error. (`crates/el-db/src/repo/notes.rs:484`)
- IPC inputs are validated at the command boundary with explicit size caps
  and enum allowlists, then validated again in the repository layer.
  (`src-tauri/src/commands/settings.rs:34`)
- Imported files are copied into a managed store under a generated name, and
  the original path is never stored or read again.
  (`src-tauri/src/commands/library.rs:415`)
- A managed file is deleted only after its path is canonicalized and
  confirmed to be a regular file inside the managed store.
  (`src-tauri/src/commands/library.rs:625`)
- Model output is sanitized before it is displayed, and every Markdown render
  site applies an HTML sanitizer. (`crates/el-security/src/sanitize.rs:10`)

### 3.4 Dependencies and Supply Chain

- The frontend lockfile is committed and CI installs with
  `--frozen-lockfile`. (`package.json:17`)
- The sidecar requirements files are pinned per environment and CI installs
  them from the committed requirements file.
- Rust dependency versions are pinned in the workspace manifest where the
  upstream package is pre-1.0 or known to be fragile. (`Cargo.toml:32`)
- CI runs the frontend linter, the type checker, the Rust formatter, Clippy
  with warnings treated as errors, and the Rust and Python test suites on
  every pull request. (`.github/workflows/ci.yml:12`)

### 3.5 Logging and Monitoring

- Diagnostics are written through a structured logging framework to a local
  SQLite file and to the console. (`src-tauri/src/logging.rs:59`)
- Key material is never logged. Startup diagnostics record the provider name,
  whether a model is set, and whether a key is present, nothing more.
- The log file is pruned on startup to the most recent 14 days and 5,000
  rows, so it cannot grow without bound. (`src-tauri/src/logging.rs:76`)
- The web clipper refuses non-HTTPS addresses and private, loopback,
  link-local, and reserved hosts, and re-checks every redirect hop.
  (`sidecar/endlessli_sidecar/clip.py:74`)
- Untrusted web text is stripped of invisible characters and delimited in the
  prompt as data rather than instructions.
  (`crates/el-research/src/research.rs:314`)

### 3.6 Build, CI, and Release

- CI covers Windows, macOS, and Linux for the Rust workspace and the Tauri
  build. (`.github/workflows/ci.yml:42`)
- The renderer runs inside a webview with a restrictive content security
  policy, and the Tauri capability surface grants a scoped external-link
  opener that accepts only `http`, `https`, `mailto`, and `tel`.
  (`src-tauri/tauri.conf.json:22`)

## 4. Known Issues

| ID | Issue (General Terms) | Affected | Mitigation | Planned Fix |
| --- | --- | --- | --- | --- |
| S-001 | The script network toggle blocks ordinary socket use but is enforced inside the interpreter rather than by the operating system. A determined script can bypass it and runs with the user's own permissions. | All versions | The toggle is off by default, the interface states the limitation before the first use, and only trusted scripts should be run. See [docs/security.md](./docs/security.md). | Operating-system-level confinement (Windows Job Objects, macOS sandbox profiles, Linux Landlock or seccomp) is a future direction. |
| S-002 | A hostile page can influence the wording of a research summary even though the prompt delimits it as data. | All versions | Nothing from a research run is saved until the user selects results and saves them. Citations are validated against the source list. Treat research output editorially. | Retrieval-side improvements to reduce the influence of untrusted text are ongoing work. |

> No other known unpatched vulnerabilities in supported versions as of
> 2026-10-20. This statement covers the default configuration. A build with
> the `sqlcipher` feature, a custom provider endpoint, or user-authored
> scripts changes the picture, and section 3 applies to those configurations
> as well.

## 5. Security History

The per-release detail lives in the CHANGELOG. The entries below point to the
changes that matter for a user who needs to know whether an upgrade changes
the security posture.

- [1.0.0, 2026-10-20]: the plaintext API-key fallback was removed, the
  external-link capability was scoped to the default URL schemes, the content
  security policy was tightened, the web clipper gained a private-address
  guard, untrusted web content was delimited in research prompts, and
  per-project note encryption was added. See the CHANGELOG Security section.

## 6. Out of Scope

- Unsupported version lines. See the table in section 1.
- Issues in third-party services, including language-model and search
  providers. Report those to the provider.
- Issues that require the user to run an untrusted script they wrote or
  received from elsewhere.
- Physical access to the device, or a compromised operating system or
  keychain.
- Denial of service through a database far larger than the reference
  baseline. See [docs/performance.md](./docs/performance.md).
- Social engineering of the user.
