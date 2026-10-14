mod common;

use common::TestDb;
use el_core::{NewFolder, NewProject};
use el_db::repo::{folders, projects};

fn new_project(name: &str) -> NewProject {
    NewProject {
        name: name.to_string(),
        description: None,
        icon: None,
        color: None,
        system_prompt: None,
    }
}

#[test]
fn project_crud_and_archive() {
    let test = TestDb::setup();
    let conn = test.conn();

    let created = projects::create(&conn, &new_project("Research")).unwrap();
    assert_eq!(created.name, "Research");
    assert!(!created.archived);

    let fetched = projects::require(&conn, &created.id).unwrap();
    assert_eq!(fetched, created);

    let updated = projects::update(
        &conn,
        &created.id,
        Some("Deep Research"),
        Some(Some("desc")),
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(updated.name, "Deep Research");
    assert_eq!(updated.description.as_deref(), Some("desc"));

    let visible = projects::list(&conn, false).unwrap();
    assert_eq!(visible.len(), 1);

    projects::set_archived(&conn, &created.id, true).unwrap();
    assert!(projects::list(&conn, false).unwrap().is_empty());
    assert_eq!(projects::list(&conn, true).unwrap().len(), 1);

    projects::delete(&conn, &created.id).unwrap();
    assert!(projects::get(&conn, &created.id).unwrap().is_none());
    assert!(projects::delete(&conn, &created.id).is_err());

    drop(conn);
    test.finish();
}

#[test]
fn project_name_must_not_be_blank() {
    let test = TestDb::setup();
    let conn = test.conn();
    assert!(projects::create(&conn, &new_project("   ")).is_err());
    drop(conn);
    test.finish();
}

#[test]
fn folders_nest_move_and_reject_cycles() {
    let test = TestDb::setup();
    let conn = test.conn();
    let project = projects::create(&conn, &new_project("P")).unwrap();

    let root = folders::create(
        &conn,
        &NewFolder {
            name: "root".to_string(),
            project_id: project.id.clone(),
            parent_id: None,
        },
    )
    .unwrap();
    let child = folders::create(
        &conn,
        &NewFolder {
            name: "child".to_string(),
            project_id: project.id.clone(),
            parent_id: Some(root.id.clone()),
        },
    )
    .unwrap();
    assert_eq!(
        folders::list_by_project(&conn, &project.id).unwrap().len(),
        2
    );

    // Moving root under its own child would cycle.
    assert!(folders::move_folder(&conn, &root.id, Some(&child.id)).is_err());
    // A folder cannot parent itself.
    assert!(folders::move_folder(&conn, &root.id, Some(&root.id)).is_err());

    // Moving the child back to the project root works.
    let moved = folders::move_folder(&conn, &child.id, None).unwrap();
    assert!(moved.parent_id.is_none());

    let renamed = folders::rename(&conn, &child.id, "renamed").unwrap();
    assert_eq!(renamed.name, "renamed");

    drop(conn);
    test.finish();
}

#[test]
fn folder_parent_must_share_project() {
    let test = TestDb::setup();
    let conn = test.conn();
    let a = projects::create(&conn, &new_project("A")).unwrap();
    let b = projects::create(&conn, &new_project("B")).unwrap();
    let root = folders::create(
        &conn,
        &NewFolder {
            name: "root".to_string(),
            project_id: a.id.clone(),
            parent_id: None,
        },
    )
    .unwrap();
    assert!(folders::create(
        &conn,
        &NewFolder {
            name: "x".to_string(),
            project_id: b.id.clone(),
            parent_id: Some(root.id.clone()),
        },
    )
    .is_err());
    // Unknown project fails through the foreign key.
    assert!(folders::create(
        &conn,
        &NewFolder {
            name: "y".to_string(),
            project_id: "missing".to_string(),
            parent_id: None,
        },
    )
    .is_err());
    drop(conn);
    test.finish();
}

#[test]
fn deleting_project_removes_folders() {
    let test = TestDb::setup();
    let conn = test.conn();
    let project = projects::create(&conn, &new_project("P")).unwrap();
    let folder = folders::create(
        &conn,
        &NewFolder {
            name: "f".to_string(),
            project_id: project.id.clone(),
            parent_id: None,
        },
    )
    .unwrap();
    projects::delete(&conn, &project.id).unwrap();
    assert!(folders::get(&conn, &folder.id).unwrap().is_none());
    drop(conn);
    test.finish();
}
