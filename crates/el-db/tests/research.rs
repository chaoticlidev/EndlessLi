mod common;

use common::TestDb;
use el_core::{NewNote, NewResult, NewSession, NewStep};
use el_db::repo::{notes, research};

fn session(conn: &el_db::DbConn) -> el_core::ResearchSession {
    research::create_session(
        conn,
        &NewSession {
            query: "what is semantic compression".to_string(),
            mode: "deep".to_string(),
            project_id: None,
            folder_id: None,
            context_memory: None,
        },
    )
    .unwrap()
}

#[test]
fn session_lifecycle_and_history() {
    let test = TestDb::setup();
    let conn = test.conn();
    let s = session(&conn);
    assert_eq!(s.status, "pending");
    assert!(research::create_session(
        &conn,
        &NewSession {
            query: "x".to_string(),
            mode: "wide".to_string(),
            project_id: None,
            folder_id: None,
            context_memory: None,
        }
    )
    .is_err());

    let running = research::set_session_status(&conn, &s.id, "in_progress").unwrap();
    assert!(running.completed_at.is_none());
    let done = research::set_session_status(&conn, &s.id, "completed").unwrap();
    assert!(done.completed_at.is_some());

    let history = research::list_sessions(&conn, None, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert!(
        research::list_sessions(&conn, Some("completed"), 10)
            .unwrap()
            .len()
            == 1
    );
    assert!(research::list_sessions(&conn, Some("failed"), 10)
        .unwrap()
        .is_empty());

    drop(conn);
    test.finish();
}

#[test]
fn steps_and_results_keep_order() {
    let test = TestDb::setup();
    let conn = test.conn();
    let s = session(&conn);

    for (i, description) in ["plan", "search", "synthesize"].iter().enumerate() {
        let step = research::add_step(
            &conn,
            &NewStep {
                session_id: s.id.clone(),
                parent_step_id: None,
                description: description.to_string(),
                status: "pending".to_string(),
                details: None,
            },
        )
        .unwrap();
        assert_eq!(step.step_order, i as i64);
    }
    let child = research::add_step(
        &conn,
        &NewStep {
            session_id: s.id.clone(),
            parent_step_id: None,
            description: "child".to_string(),
            status: "pending".to_string(),
            details: None,
        },
    )
    .unwrap();
    research::set_step_status(&conn, &child.id, "completed").unwrap();

    // Nested steps order within their own parent scope.
    let nested = research::add_step(
        &conn,
        &NewStep {
            session_id: s.id.clone(),
            parent_step_id: Some(child.id.clone()),
            description: "nested".to_string(),
            status: "pending".to_string(),
            details: None,
        },
    )
    .unwrap();
    assert_eq!(nested.step_order, 0);
    assert_eq!(research::list_steps(&conn, &s.id).unwrap().len(), 5);

    let first = research::add_result(
        &conn,
        &NewResult {
            session_id: s.id.clone(),
            result_type: "summary".to_string(),
            content: "summary text".to_string(),
            source_url: None,
            source_title: None,
        },
    )
    .unwrap();
    let second = research::add_result(
        &conn,
        &NewResult {
            session_id: s.id.clone(),
            result_type: "source".to_string(),
            content: "source text".to_string(),
            source_url: Some("https://example.com".to_string()),
            source_title: Some("Example".to_string()),
        },
    )
    .unwrap();
    assert!(second.result_order > first.result_order);
    research::set_result_selected(&conn, &second.id, true).unwrap();
    let results = research::list_results(&conn, &s.id).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[1].selected_for_note);

    drop(conn);
    test.finish();
}

#[test]
fn delete_session_cascades_children_but_keeps_notes() {
    let test = TestDb::setup();
    let conn = test.conn();
    let s = session(&conn);
    research::add_step(
        &conn,
        &NewStep {
            session_id: s.id.clone(),
            parent_step_id: None,
            description: "search".to_string(),
            status: "completed".to_string(),
            details: None,
        },
    )
    .unwrap();
    research::add_result(
        &conn,
        &NewResult {
            session_id: s.id.clone(),
            result_type: "summary".to_string(),
            content: "summary".to_string(),
            source_url: None,
            source_title: None,
        },
    )
    .unwrap();
    let n = notes::create(
        &conn,
        &NewNote {
            title: "saved".to_string(),
            content: "body".to_string(),
            project_id: None,
            folder_id: None,
            author: "ai".to_string(),
            source_session_id: Some(s.id.clone()),
            created_from_note_id: None,
        },
    )
    .unwrap();
    research::attach_note(&conn, &s.id, &n.id).unwrap();

    research::delete_session(&conn, &s.id).unwrap();

    assert!(research::get_session(&conn, &s.id).unwrap().is_none());
    assert!(research::list_steps(&conn, &s.id).unwrap().is_empty());
    assert!(research::list_results(&conn, &s.id).unwrap().is_empty());

    // The note survives and loses only its session back-reference.
    let kept = notes::get(&conn, &n.id).unwrap().expect("note was deleted");
    assert_eq!(kept.title, "saved");
    assert!(kept.source_session_id.is_none());

    assert!(research::delete_session(&conn, &s.id).is_err());

    drop(conn);
    test.finish();
}

#[test]
fn session_notes_and_token_accounting() {
    let test = TestDb::setup();
    let conn = test.conn();
    let s = session(&conn);
    let n = notes::create(
        &conn,
        &NewNote {
            title: "saved".to_string(),
            content: "body".to_string(),
            project_id: None,
            folder_id: None,
            author: "ai".to_string(),
            source_session_id: Some(s.id.clone()),
            created_from_note_id: None,
        },
    )
    .unwrap();
    research::attach_note(&conn, &s.id, &n.id).unwrap();
    research::attach_note(&conn, &s.id, &n.id).unwrap();
    assert_eq!(research::notes_for_session(&conn, &s.id).unwrap().len(), 1);

    let mut write = test.db.connect().unwrap();
    research::add_tokens(&mut write, &s.id, "openai", "gpt-x", 100, 50).unwrap();
    research::add_tokens(&mut write, &s.id, "openai", "gpt-x", 25, 25).unwrap();
    drop(write);
    let updated = research::require_session(&conn, &s.id).unwrap();
    assert_eq!((updated.tokens_in, updated.tokens_out), (125, 75));

    assert!(
        research::add_tokens(&mut test.db.connect().unwrap(), "missing", "p", "m", 1, 1).is_err()
    );

    drop(conn);
    test.finish();
}
