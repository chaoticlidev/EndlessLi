mod common;

use common::TestDb;
use el_core::{NewMemoryEntry, NewProject};
use el_db::repo::{memory, projects};

fn entry(memory_type: &str, project: Option<&str>, content: &str) -> NewMemoryEntry {
    NewMemoryEntry {
        memory_type: memory_type.to_string(),
        project_id: project.map(str::to_string),
        content: content.to_string(),
        source: "user".to_string(),
        expires_at: None,
    }
}

#[test]
fn memory_scopes_and_forget() {
    let test = TestDb::setup();
    let conn = test.conn();
    let p = projects::create(
        &conn,
        &NewProject {
            name: "P".to_string(),
            description: None,
            icon: None,
            color: None,
            system_prompt: None,
        },
    )
    .unwrap();

    memory::create_entry(&conn, &entry("global", None, "likes concise answers")).unwrap();
    memory::create_entry(&conn, &entry("project", Some(&p.id), "usesRust")).unwrap();

    // Scope mismatches are rejected with a clear message.
    assert!(memory::create_entry(&conn, &entry("global", Some(&p.id), "x")).is_err());
    assert!(memory::create_entry(&conn, &entry("project", None, "x")).is_err());

    assert_eq!(memory::list_global(&conn).unwrap().len(), 1);
    assert_eq!(memory::list_for_project(&conn, &p.id).unwrap().len(), 1);

    let e = &memory::list_global(&conn).unwrap()[0];
    memory::forget(&conn, &e.id).unwrap();
    assert!(memory::list_global(&conn).unwrap().is_empty());
    // Soft delete keeps the row.
    assert!(memory::get_entry(&conn, &e.id).unwrap().is_some());
    memory::delete_permanent(&conn, &e.id).unwrap();
    assert!(memory::get_entry(&conn, &e.id).unwrap().is_none());

    drop(conn);
    test.finish();
}

#[test]
fn expired_memories_sweep() {
    let test = TestDb::setup();
    let conn = test.conn();
    let past = Some(1_i64);
    let e = memory::create_entry(
        &conn,
        &NewMemoryEntry {
            expires_at: past,
            ..entry("global", None, "temporary")
        },
    )
    .unwrap();
    assert_eq!(memory::sweep_expired(&conn).unwrap(), 1);
    let fetched = memory::get_entry(&conn, &e.id).unwrap().unwrap();
    assert!(!fetched.is_active);

    drop(conn);
    test.finish();
}

#[test]
fn suggestions_require_approval() {
    let test = TestDb::setup();
    let conn = test.conn();

    let s = memory::suggest(&conn, None, "prefers examples", None).unwrap();
    assert_eq!(memory::list_suggestions(&conn).unwrap().len(), 1);
    assert!(memory::list_global(&conn).unwrap().is_empty());

    let mut write = test.db.connect().unwrap();
    let entry = memory::accept_suggestion(&mut write, &s.id).unwrap();
    drop(write);
    assert_eq!(entry.source, "ai_suggested");
    assert_eq!(entry.memory_type, "global");
    assert!(memory::list_suggestions(&conn).unwrap().is_empty());
    assert_eq!(memory::list_global(&conn).unwrap().len(), 1);

    let doomed = memory::suggest(&conn, None, "never mind", None).unwrap();
    memory::reject_suggestion(&conn, &doomed.id).unwrap();
    assert!(memory::list_suggestions(&conn).unwrap().is_empty());

    drop(conn);
    test.finish();
}

#[test]
fn memory_kill_switch() {
    let test = TestDb::setup();
    let conn = test.conn();
    assert!(memory::is_enabled(&conn).unwrap());
    memory::set_enabled(&conn, false).unwrap();
    assert!(!memory::is_enabled(&conn).unwrap());
    memory::set_enabled(&conn, true).unwrap();
    assert!(memory::is_enabled(&conn).unwrap());
    drop(conn);
    test.finish();
}
