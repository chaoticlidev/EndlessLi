//! Read-through semantics: hits never compute, misses compute exactly once,
//! expiry recomputes, and pressure evicts.

use el_cache::{get_or_compute, CacheInput, CacheValue};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct TestDb {
    db: el_db::Database,
    path: std::path::PathBuf,
}

impl TestDb {
    fn setup() -> Self {
        let (db, path) = el_db::Database::open_ephemeral().expect("open test database");
        Self { db, path }
    }

    fn finish(self) {
        drop(self.db);
        std::fs::remove_file(self.path).ok();
    }
}

fn input(query: &str) -> CacheInput {
    CacheInput {
        entry_type: "search",
        query: query.to_string(),
        model: "ddg".to_string(),
        source_type: "ddg".to_string(),
        ttl_secs: None,
    }
}

#[tokio::test]
async fn repeated_query_makes_zero_compute_calls() {
    let test = TestDb::setup();
    let calls = Arc::new(AtomicUsize::new(0));

    let mut hits = Vec::new();
    for _ in 0..3 {
        let counter = calls.clone();
        let (bytes, hit) = get_or_compute(&test.db, input("rust borrowing"), || async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(CacheValue {
                bytes: b"results".to_vec(),
                source_type: "ddg".to_string(),
            })
        })
        .await
        .expect("lookup");
        assert_eq!(bytes, b"results");
        hits.push(hit);
    }
    // One miss followed by two pure hits.
    assert_eq!(hits, vec![false, true, true]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let conn = test.db.connect().expect("connect");
    let cached = el_db::repo::cache::get(&conn, "search", "rust borrowing", "ddg")
        .expect("get")
        .expect("stored");
    assert!(cached.response == b"results");
    drop(conn);
    test.finish();
}

#[tokio::test]
async fn expired_entries_recompute() {
    let test = TestDb::setup();
    let conn = test.db.connect().expect("connect");
    el_db::repo::cache::put(
        &conn,
        "search",
        "stale query",
        "ddg",
        b"old",
        "ddg",
        Some(-5),
    )
    .expect("seed expired");
    drop(conn);

    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let (bytes, hit) = get_or_compute(&test.db, input("stale query"), || async move {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(CacheValue {
            bytes: b"fresh".to_vec(),
            source_type: "ddg".to_string(),
        })
    })
    .await
    .expect("lookup");
    assert!(!hit);
    assert_eq!(bytes, b"fresh");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    test.finish();
}

#[tokio::test]
async fn misses_stay_usable_under_pressure() {
    let test = TestDb::setup();
    let conn = test.db.connect().expect("connect");
    el_db::repo::cache::set_setting(&conn, el_db::repo::cache::SETTING_CACHE_MAX_BYTES, "600")
        .expect("set ceiling");
    drop(conn);

    // Each response is ~200 bytes against a 600-byte ceiling: every miss
    // stores, eviction trims the oldest, and lookups keep working.
    for i in 0..8 {
        let (bytes, _) = get_or_compute(&test.db, input(&format!("page {i}")), || async move {
            Ok(CacheValue {
                bytes: vec![b'x'; 200],
                source_type: "clip".to_string(),
            })
        })
        .await
        .expect("lookup");
        assert_eq!(bytes.len(), 200);
    }
    let conn = test.db.connect().expect("connect");
    let stats = el_db::repo::cache::stats(&conn).expect("stats");
    assert!(stats.bytes <= 600, "bytes after eviction: {}", stats.bytes);
    assert!(stats.entries > 0);
    drop(conn);
    test.finish();
}

#[tokio::test]
async fn compute_errors_are_not_cached() {
    let test = TestDb::setup();
    let failing = get_or_compute(&test.db, input("boom"), || async {
        Err(el_core::AppError::Internal("network down".to_string()))
    })
    .await;
    assert!(failing.is_err());

    // A later success stores normally; the earlier error left no row.
    let (bytes, hit) = get_or_compute(&test.db, input("boom"), || async {
        Ok(CacheValue {
            bytes: b"recovered".to_vec(),
            source_type: "ddg".to_string(),
        })
    })
    .await
    .expect("lookup");
    assert!(!hit);
    assert_eq!(bytes, b"recovered");
    test.finish();
}
