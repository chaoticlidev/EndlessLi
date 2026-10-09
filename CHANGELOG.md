# Changelog

All notable changes to EndlessLi are documented here, following Keep a
Changelog. Versioning is two-part (MAJOR.MINOR).

## [0.1.0] - 10-08-2026

## Summary

Initial project foundation completed

### Added

- Initial project foundation: Tauri 2 + React + TypeScript (strict) + Tailwind CSS v4
  scaffold with placeholder App shell.
- Cargo workspace (`src-tauri` + `crates/el-*`): `el-core`, `el-db`,
  `el-analytics` (feature `duckdb`), `el-llm`, `el-cache`, `el-memory`,
  `el-linking`, `el-research`, `el-compress`, `el-skills`, `el-security`.
- `tracing` subscriber with SQLite log layer (`logs.db`, queryable `logs`
  table) and `AppState` wiring in the Tauri setup hook.
- Python sidecar lifecycle stub (`SidecarManager` with restart cap) for
  Phase 5 to fill in.
- ESLint (flat config), Prettier, `tsc --noEmit`, `clippy -D warnings`,
  `rustfmt` check.
- CI matrix (Windows, macOS, Linux): frontend lint/typecheck/build, Rust
  fmt/clippy/test, Tauri build.
- Tauri capabilities (minimal) and CSP configuration.
- `CHANGELOG.md`, `LICENSE`, `.env.example` created.

Deviation log:

- React 19 kept from `create-tauri-app` template instead of React 18.
  Reason: template default, API-compatible for planned usage. No action needed.
- TypeScript ~5.9 pinned instead of template's ~6.0 for eslint/vite compat.
  Revisit when typescript-eslint officially supports TS 6.