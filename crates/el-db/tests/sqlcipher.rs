//! SQLCipher integration checks.
//!
//! Run with:
//! `cargo test -p el-db --features sqlcipher --test sqlcipher -- --nocapture`
//!
//! These assertions are deliberately blunt: a build that silently linked
//! plain SQLite must fail here rather than pass an encryption test that does
//! not encrypt anything.

#![cfg(feature = "sqlcipher")]

use el_db::Database;
use rusqlite::Connection;

const PASSPHRASE: &str = "endlessli-sqlcipher-test-passphrase";

#[test]
fn sqlcipher_encrypts_and_rejects_plaintext_reads() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("encrypted.db");

    let db = Database::open_with_key(&path, Some(PASSPHRASE)).expect("open encrypted db");
    let cipher_version: String = {
        let conn = db.connect().expect("checkout encrypted connection");
        let version: Option<String> = conn
            .query_row("PRAGMA cipher_version", [], |row| row.get(0))
            .ok();
        conn.execute(
            "INSERT INTO notes(id, title, content, author, created_at, updated_at)
             VALUES ('sqlcipher-probe', 'probe', 'ciphertext check', 'user', 0, 0)",
            [],
        )
        .expect("insert probe note");
        version.unwrap_or_default()
    };
    assert!(
        !cipher_version.trim().is_empty(),
        "PRAGMA cipher_version is empty; this build linked plain SQLite instead of SQLCipher"
    );
    println!("PRAGMA cipher_version = {cipher_version}");
    drop(db);

    let header = std::fs::read(&path).expect("read database file");
    assert!(
        !header.starts_with(b"SQLite format 3\0"),
        "database header is plaintext; SQLCipher did not encrypt the file"
    );

    let plain = Connection::open(&path).expect("plain SQLite opens the file lazily");
    let plain_read = plain.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    });
    assert!(
        plain_read.is_err(),
        "a plain SQLite connection read an encrypted database"
    );

    let wrong_key = Database::open_with_key(&path, Some("definitely the wrong key"));
    assert!(wrong_key.is_err(), "a wrong passphrase opened the database");

    let reopened =
        Database::open_with_key(&path, Some(PASSPHRASE)).expect("reopen with the right key");
    let title: String = reopened
        .connect()
        .expect("checkout reopened connection")
        .query_row(
            "SELECT title FROM notes WHERE id = 'sqlcipher-probe'",
            [],
            |row| row.get(0),
        )
        .expect("read probe note");
    assert_eq!(title, "probe");
}
