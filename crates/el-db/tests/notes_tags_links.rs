mod common;

use common::TestDb;
use el_core::{NewNote, NewProject, UpdateNote};
use el_db::repo::{links, notes, projects, tags};

fn project(conn: &el_db::DbConn, name: &str) -> el_core::Project {
    projects::create(
        conn,
        &NewProject {
            name: name.to_string(),
            description: None,
            icon: None,
            color: None,
            system_prompt: None,
        },
    )
    .unwrap()
}

fn note(conn: &el_db::DbConn, title: &str, project: Option<&str>) -> el_core::Note {
    notes::create(
        conn,
        &NewNote {
            title: title.to_string(),
            content: format!("content of {title}"),
            project_id: project.map(str::to_string),
            folder_id: None,
            author: "user".to_string(),
            source_session_id: None,
            created_from_note_id: None,
        },
    )
    .unwrap()
}

#[test]
fn note_crud_scopes_and_fts() {
    let test = TestDb::setup();
    let conn = test.conn();
    let p = project(&conn, "P");

    let inbox = note(&conn, "inbox note", None);
    let scoped = note(&conn, "project gardening tips", Some(&p.id));

    assert_eq!(notes::list_inbox(&conn).unwrap().len(), 1);
    assert_eq!(notes::list_by_project(&conn, &p.id).unwrap().len(), 1);

    let hits = notes::search(&conn, "gardening", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, scoped.id);

    // Queries with quote characters cannot break the search.
    let tricky = notes::search(&conn, r#"gardening" OR "1"="1"#, 10).unwrap();
    assert!(tricky.is_empty());

    // Blank titles and unknown authors are rejected.
    let blank = NewNote {
        title: "   ".to_string(),
        content: "x".to_string(),
        project_id: None,
        folder_id: None,
        author: "user".to_string(),
        source_session_id: None,
        created_from_note_id: None,
    };
    assert!(notes::create(&conn, &blank).is_err());
    let bad_author = NewNote {
        author: "ghost".to_string(),
        ..blank
    };
    assert!(notes::create(&conn, &bad_author).is_err());

    // Deleting removes the FTS row through the trigger.
    let mut conn_mut = test.db.connect().unwrap();
    notes::delete(&mut conn_mut, &scoped.id).unwrap();
    drop(conn_mut);
    assert!(notes::search(&conn, "gardening", 10).unwrap().is_empty());
    assert!(notes::get(&conn, &inbox.id).unwrap().is_some());

    drop(conn);
    test.finish();
}

#[test]
fn note_update_and_branch() {
    let test = TestDb::setup();
    let conn = test.conn();
    let tag = tags::create(&conn, "topic", None).unwrap();
    let original = note(&conn, "original", None);
    tags::assign_to_note(&conn, &original.id, &tag.id).unwrap();

    let mut write = test.db.connect().unwrap();
    let updated = notes::update(
        &mut write,
        &original.id,
        &UpdateNote {
            title: Some("renamed".to_string()),
            content: None,
            project_id: None,
            folder_id: None,
        },
    )
    .unwrap();
    assert_eq!(updated.title, "renamed");
    assert_eq!(updated.content, original.content);

    let branched = notes::branch(&mut write, &original.id, None).unwrap();
    drop(write);
    assert_eq!(
        branched.created_from_note_id.as_deref(),
        Some(original.id.as_str())
    );
    assert!(branched.title.contains("branch"));
    let branched_tags = tags::tags_for_note(&conn, &branched.id).unwrap();
    assert_eq!(branched_tags.len(), 1);
    assert_eq!(branched_tags[0].id, tag.id);

    drop(conn);
    test.finish();
}

#[test]
fn tags_are_case_insensitive_and_idempotent() {
    let test = TestDb::setup();
    let conn = test.conn();
    let n = note(&conn, "n", None);

    let a = tags::get_or_create(&conn, "Rust").unwrap();
    let b = tags::get_or_create(&conn, "rust").unwrap();
    assert_eq!(a.id, b.id);

    tags::assign_to_note(&conn, &n.id, &a.id).unwrap();
    tags::assign_to_note(&conn, &n.id, &a.id).unwrap();
    assert_eq!(tags::tags_for_note(&conn, &n.id).unwrap().len(), 1);

    // Unknown notes fail through the foreign key, not silently.
    assert!(tags::assign_to_note(&conn, "missing", &a.id).is_err());

    tags::unassign_from_note(&conn, &n.id, &a.id).unwrap();
    assert!(tags::tags_for_note(&conn, &n.id).unwrap().is_empty());

    let names: Vec<String> = tags::list(&conn)
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, vec!["Rust".to_string()]);

    drop(conn);
    test.finish();
}

#[test]
fn links_use_single_canonical_write_path() {
    let test = TestDb::setup();
    let conn = test.conn();
    let a = note(&conn, "a", None);
    let b = note(&conn, "b", None);

    // Order of arguments does not matter: one row, canonical (a < b).
    let first = links::approve(&conn, &b.id, &a.id, Some(0.9), "auto_approved").unwrap();
    assert!(first.note_a < first.note_b);
    // Second insert of the same pair conflicts instead of duplicating.
    assert!(links::approve(&conn, &a.id, &b.id, None, "manual").is_err());

    let listed = links::list_for_note(&conn, &a.id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(links::list_for_note(&conn, &b.id).unwrap().len(), 1);

    assert!(links::approve(&conn, &a.id, &a.id, None, "manual").is_err());
    assert!(links::approve(&conn, &a.id, "missing", None, "manual").is_err());

    links::remove(&conn, &a.id, &b.id).unwrap();
    assert!(links::list_for_note(&conn, &a.id).unwrap().is_empty());

    // Rejections are idempotent and canonical too.
    links::reject(&conn, &b.id, &a.id).unwrap();
    links::reject(&conn, &a.id, &b.id).unwrap();

    drop(conn);
    test.finish();
}
