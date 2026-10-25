//! Phase 11.4 performance baseline: 10k notes.
//!
//! These are synthetic upper-bound checks, not micro-benchmarks. They seed a
//! throwaway database once, print the timings, and fail only on egregious
//! regressions so slow CI hardware does not produce false negatives. Run with
//! `cargo test -p el-db --test performance -- --nocapture` to see the table.

use std::time::Instant;

use el_core::NewNote;
use el_db::repo::notes;

fn note(index: usize) -> NewNote {
    NewNote {
        title: format!("Benchmark note {index}"),
        content: format!(
            "This is benchmark note {index}. It contains the searchable needle word. \
             Additional prose keeps the row realistic without making the fixture huge."
        ),
        project_id: None,
        folder_id: None,
        author: "user".to_string(),
        source_session_id: None,
        created_from_note_id: None,
    }
}

#[test]
fn ten_thousand_note_baseline() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("perf.db");

    let start = Instant::now();
    let db = el_db::Database::open(&path).expect("open database");
    let open_ms = start.elapsed().as_millis();

    let start = Instant::now();
    db.transaction(|tx| {
        for index in 0..10_000 {
            notes::create(tx, &note(index))?;
        }
        Ok(())
    })
    .expect("seed notes");
    let seed_ms = start.elapsed().as_millis();

    let conn = db.connect().expect("connection");

    let start = Instant::now();
    let listed = notes::list_all(&conn).expect("list notes");
    let list_ms = start.elapsed().as_millis();
    assert_eq!(listed.len(), 10_000);

    let start = Instant::now();
    let hits = notes::search(&conn, "needle", 100).expect("search notes");
    let search_ms = start.elapsed().as_millis();
    assert_eq!(hits.len(), 100);

    let start = Instant::now();
    let fetched = notes::get(&conn, &listed[0].id)
        .expect("get note")
        .expect("note exists");
    let get_us = start.elapsed().as_micros();
    assert_eq!(fetched.id, listed[0].id);

    println!(
        "\n10k-note baseline (debug build):\n\
         | operation | time |\n\
         | --- | --- |\n\
         | open + migrate | {open_ms} ms |\n\
         | seed 10k notes | {seed_ms} ms |\n\
         | list all (full rows) | {list_ms} ms |\n\
         | FTS search (100 hits) | {search_ms} ms |\n\
         | get one note | {get_us} us |"
    );

    // Generous ceilings: correctness/regression guards, not hardware claims.
    assert!(open_ms < 10_000, "database open took {open_ms} ms");
    assert!(list_ms < 5_000, "list_all took {list_ms} ms");
    assert!(search_ms < 1_000, "FTS search took {search_ms} ms");
    assert!(get_us < 250_000, "get took {get_us} us");
}
