mod common;

use common::TestDb;
use el_db::repo::cache;

#[test]
fn cache_key_is_deterministic() {
    assert_eq!(
        cache::cache_key("search", "ddg", "  Hello   World "),
        cache::cache_key("search", "ddg", "hello world")
    );
    assert_ne!(
        cache::cache_key("search", "ddg", "hello world"),
        cache::cache_key("search", "brave", "hello world")
    );
    assert_ne!(
        cache::cache_key("search", "ddg", "hello world"),
        cache::cache_key("webpage", "ddg", "hello world")
    );
}

#[test]
fn cache_put_get_expiry_and_stats() {
    let test = TestDb::setup();
    let conn = test.conn();

    assert!(cache::get(&conn, "search", "rust", "ddg")
        .unwrap()
        .is_none());

    let stored = cache::put(
        &conn,
        "search",
        "rust",
        "ddg",
        b"result-bytes",
        "ddg",
        cache::default_ttl("search"),
    )
    .unwrap();
    assert_eq!(stored.response, b"result-bytes");
    assert_eq!(stored.access_count, 1);

    let hit = cache::get(&conn, "search", "rust", "ddg").unwrap().unwrap();
    assert_eq!(hit.response, b"result-bytes");
    assert_eq!(hit.access_count, 2);

    // Expired rows read as misses and are removed.
    cache::put(&conn, "search", "old", "ddg", b"x", "ddg", Some(-5)).unwrap();
    assert!(cache::get(&conn, "search", "old", "ddg").unwrap().is_none());

    let stats = cache::stats(&conn).unwrap();
    assert_eq!(stats.entries, 1);

    assert_eq!(cache::clear_by_type(&conn, "search").unwrap(), 1);
    assert_eq!(cache::stats(&conn).unwrap().entries, 0);
    assert_eq!(cache::clear_all(&conn).unwrap(), 0);

    drop(conn);
    test.finish();
}

#[test]
fn cache_evicts_lru_under_pressure() {
    let test = TestDb::setup();
    let conn = test.conn();
    cache::set_setting(&conn, cache::SETTING_CACHE_MAX_BYTES, "600").unwrap();

    // Each row is ~200 bytes; 5 rows exceed the 600-byte ceiling.
    for i in 0..5 {
        cache::put(
            &conn,
            "webpage",
            &format!("page {i}"),
            "clip",
            &[b'x'; 200],
            "clip",
            None,
        )
        .unwrap();
    }
    let stats = cache::stats(&conn).unwrap();
    assert!(stats.bytes <= 600, "bytes after eviction: {}", stats.bytes);

    drop(conn);
    test.finish();
}

#[test]
fn token_usage_and_settings() {
    let test = TestDb::setup();
    let conn = test.conn();

    cache::log_usage(&conn, Some("s1"), "openai", "gpt-x", 10, 5).unwrap();
    cache::log_usage(&conn, None, "anthropic", "claude", 7, 3).unwrap();
    assert!(cache::log_usage(&conn, None, "p", "m", -1, 0).is_err());

    let (tin, tout) = cache::sum_since(&conn, 0).unwrap();
    assert_eq!((tin, tout), (17, 8));
    assert_eq!(cache::sum_for_session(&conn, "s1").unwrap(), (10, 5));
    assert_eq!(cache::sum_since(&conn, i64::MAX).unwrap(), (0, 0));

    assert!(cache::get_setting(&conn, "theme").unwrap().is_none());
    cache::set_setting(&conn, "theme", r#""dark""#).unwrap();
    assert_eq!(
        cache::get_setting(&conn, "theme").unwrap().as_deref(),
        Some(r#""dark""#)
    );
    cache::delete_setting(&conn, "theme").unwrap();
    assert!(cache::get_setting(&conn, "theme").unwrap().is_none());

    drop(conn);
    test.finish();
}
