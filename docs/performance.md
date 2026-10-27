# Performance Baseline

This document records the repeatable baseline for a 10,000-note database and how to reproduce it on your own machine. The numbers below are measured, not estimated, and the command that produced them is given in full.

## Reproduce

```powershell
cargo test -p el-db --test performance -- --nocapture
```

The test creates a throwaway SQLite database, seeds 10,000 notes (title + content containing a searchable term), and prints:

| Operation | What It Measures |
| --- | --- |
| Open + Migrate | `Database::open`, migrations, pragma verification |
| Seed 10k notes | 10,000 inserts in one transaction |
| List all (full rows) | `note_list` / `SELECT * ORDER BY updated_at DESC` |
| FTS search | `notes_fts MATCH "needle" LIMIT 100` |
| Get one note | Primary-key lookup |

The assertions are deliberately generous regression guards (open < 10 s, list < 5 s, search < 1 s, get < 250 ms) so slow CI machines do not fail. Read the printed numbers rather than treating the test as a hardware benchmark.

## Baseline

Windows development machine, debug build, throwaway database:

| Operation | Time |
| --- | --- |
| Open + Migrate | 471 ms |
| Seed 10,000 notes (one transaction) | 4,192 ms |
| List all full rows | 99 ms |
| FTS search, 100 hits | 60 ms |
| Get one note | 240 µs |

The open/migrate figure is the startup database cost, not the full window launch. A release build is faster than a debug build. Process-to-first-frame is a separate measurement and is not covered by this test.

## Interpreting the Results

- The benchmark runs in a **debug** build. Release builds are substantially faster (optimized SQLite, LTO, one codegen unit).
- Startup opens the database and runs idempotent migrations; it does not load note rows. The Knowledge Base requests `note_list` when opened, which returns full note rows. For very large databases this is the first candidate for pagination or a metadata-only list command.
- FTS search is phrase-wrapped and index-backed. Search excludes encrypted projects by design (their content is ciphertext).
- Logs are pruned on startup (14 days or 5,000 rows) so `logs.db` growth does not affect startup over time.
