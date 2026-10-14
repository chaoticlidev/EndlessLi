mod common;

use common::TestDb;
use el_core::NewNote;
use el_db::repo::{links, notes};
use el_db::vec;

const MODEL: &str = "test-model";
const DIM: usize = 4;

fn note(conn: &el_db::DbConn, title: &str) -> el_core::Note {
    notes::create(
        conn,
        &NewNote {
            title: title.to_string(),
            content: format!("content of {title}"),
            project_id: None,
            folder_id: None,
            author: "user".to_string(),
            source_session_id: None,
            created_from_note_id: None,
        },
    )
    .unwrap()
}

fn embedded(conn: &el_db::DbConn, title: &str, vector: &[f32]) -> el_core::Note {
    let n = note(conn, title);
    let hash = format!("hash-{title}");
    vec::upsert_embedding(conn, &n.id, MODEL, &hash, vector).unwrap();
    n
}

#[test]
fn vectors_insert_query_and_exclude() {
    let test = TestDb::setup();
    let conn = test.conn();
    let mut write = test.db.connect().unwrap();
    let sync = vec::sync_embeddings_table(&mut write, MODEL, DIM).unwrap();
    assert!(sync.recreated);
    drop(write);

    let a = embedded(&conn, "a", &[1.0, 0.0, 0.0, 0.0]);
    let b = embedded(&conn, "b", &[0.9, 0.1, 0.0, 0.0]);
    let c = embedded(&conn, "c", &[0.0, 1.0, 0.0, 0.0]);

    // Nearest to A (excluding A itself) is B, then C.
    let hits = vec::find_similar(&conn, &[1.0, 0.0, 0.0, 0.0], &a.id, MODEL, 2.0, 5).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].note_id, b.id);
    assert_eq!(hits[1].note_id, c.id);
    assert!(hits[0].distance < hits[1].distance);

    // A tight ceiling filters the distant note out.
    let tight = vec::find_similar(&conn, &[1.0, 0.0, 0.0, 0.0], &a.id, MODEL, 0.01, 5).unwrap();
    assert_eq!(tight.len(), 1);

    // Linked and rejected notes never surface.
    links::approve(&conn, &a.id, &b.id, None, "manual").unwrap();
    links::reject(&conn, &a.id, &c.id).unwrap();
    let filtered = vec::find_similar(&conn, &[1.0, 0.0, 0.0, 0.0], &a.id, MODEL, 2.0, 5).unwrap();
    assert!(filtered.is_empty());

    drop(conn);
    test.finish();
}

#[test]
fn vector_dimension_mismatch_rejected() {
    let test = TestDb::setup();
    let conn = test.conn();
    let mut write = test.db.connect().unwrap();
    vec::sync_embeddings_table(&mut write, MODEL, DIM).unwrap();
    drop(write);

    let n = note(&conn, "a");
    assert!(vec::upsert_embedding(&conn, &n.id, MODEL, "h", &[1.0, 0.0]).is_err());
    assert!(vec::find_similar(&conn, &[1.0], &n.id, MODEL, 2.0, 5).is_err());
    assert!(vec::upsert_embedding(&conn, "missing", MODEL, "h", &[1.0, 0.0, 0.0, 0.0]).is_err());

    drop(conn);
    test.finish();
}

#[test]
fn model_switch_recreates_table_and_marks_stale() {
    let test = TestDb::setup();
    let conn = test.conn();
    let mut write = test.db.connect().unwrap();
    vec::sync_embeddings_table(&mut write, MODEL, DIM).unwrap();
    drop(write);

    let n = embedded(&conn, "a", &[1.0, 0.0, 0.0, 0.0]);
    let stored = notes::require(&conn, &n.id).unwrap();
    assert_eq!(stored.embedding_model.as_deref(), Some(MODEL));

    // Same model and dimension is a no-op.
    let mut write = test.db.connect().unwrap();
    let same = vec::sync_embeddings_table(&mut write, MODEL, DIM).unwrap();
    assert!(!same.recreated);
    drop(write);

    // A new model drops the table and clears every embedding reference.
    let mut write = test.db.connect().unwrap();
    let switched = vec::sync_embeddings_table(&mut write, "new-model", DIM).unwrap();
    assert!(switched.recreated);
    drop(write);
    let stale = notes::require(&conn, &n.id).unwrap();
    assert!(stale.embedding_model.is_none());
    assert!(stale.embedding_hash.is_none());

    // Old vectors are gone; new ones work under the new model.
    vec::upsert_embedding(&conn, &n.id, "new-model", "h2", &[0.0, 1.0, 0.0, 0.0]).unwrap();

    drop(conn);
    test.finish();
}
