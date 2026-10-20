mod common;

use common::TestDb;
use el_core::{NewLibraryItem, NewScript, NewSkill};
use el_db::repo::{library, skills, tags};

fn item(title: &str, content: &str) -> NewLibraryItem {
    NewLibraryItem {
        item_type: "article".to_string(),
        title: title.to_string(),
        authors: Some(r#"["Ada Lovelace"]"#.to_string()),
        url: Some("https://example.com/a".to_string()),
        source: Some("Example".to_string()),
        publication_date: None,
        content: Some(content.to_string()),
        file_path: None,
    }
}

#[test]
fn library_crud_search_and_tags() {
    let test = TestDb::setup();
    let conn = test.conn();

    assert!(library::create(
        &conn,
        &NewLibraryItem {
            item_type: "scroll".to_string(),
            ..item("bad", "bad")
        }
    )
    .is_err());

    let a = library::create(
        &conn,
        &item("Rust borrow checker", "ownership and borrowing rules"),
    )
    .unwrap();
    library::create(&conn, &item("Baking bread", "flour water salt yeast")).unwrap();

    assert_eq!(library::list(&conn, None).unwrap().len(), 2);
    assert_eq!(library::list(&conn, Some("article")).unwrap().len(), 2);
    assert!(library::list(&conn, Some("pdf")).unwrap().is_empty());

    let hits = library::search(&conn, "borrowing", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, a.id);

    let updated = library::update(
        &conn,
        &a.id,
        &el_core::UpdateLibraryItem {
            title: Some("Rust ownership".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(updated.title, "Rust ownership");

    let tag = tags::create(&conn, "reading", None).unwrap();
    library::assign_tag(&conn, &a.id, &tag.id).unwrap();
    library::assign_tag(&conn, &a.id, &tag.id).unwrap();
    assert_eq!(library::tags_for_item(&conn, &a.id).unwrap().len(), 1);

    library::delete(&conn, &a.id).unwrap();
    assert!(library::search(&conn, "borrowing", 10).unwrap().is_empty());

    drop(conn);
    test.finish();
}

#[test]
fn library_filters_stats_sources_and_linking() {
    let test = TestDb::setup();
    let conn = test.conn();

    let a = library::create(
        &conn,
        &NewLibraryItem {
            item_type: "article".to_string(),
            title: "Rust ownership".to_string(),
            authors: None,
            url: Some("https://example.com/rust".to_string()),
            source: Some("Example".to_string()),
            publication_date: Some("2024".to_string()),
            content: Some("ownership borrowing lifetimes".to_string()),
            file_path: None,
        },
    )
    .unwrap();
    let b = library::create(
        &conn,
        &NewLibraryItem {
            item_type: "pdf".to_string(),
            title: "Bread baking".to_string(),
            authors: None,
            url: None,
            source: Some("Kitchen".to_string()),
            publication_date: None,
            content: Some("flour water salt yeast".to_string()),
            file_path: Some("/data/library/x.pdf".to_string()),
        },
    )
    .unwrap();

    // No query lists every item; a type filter narrows it.
    assert_eq!(
        library::search_filtered(&conn, None, None, None, None, 10)
            .unwrap()
            .len(),
        2
    );
    let pdfs = library::search_filtered(&conn, None, Some("pdf"), None, None, 10).unwrap();
    assert_eq!(pdfs.len(), 1);
    assert_eq!(pdfs[0].id, b.id);
    assert!(library::search_filtered(&conn, None, Some("scroll"), None, None, 10).is_err());

    // Source filter.
    let by_source = library::search_filtered(&conn, None, None, None, Some("Example"), 10).unwrap();
    assert_eq!(by_source.len(), 1);
    assert_eq!(by_source[0].id, a.id);

    // Tag filter.
    let tag = tags::create(&conn, "cooking", None).unwrap();
    library::assign_tag(&conn, &b.id, &tag.id).unwrap();
    let tagged = library::search_filtered(&conn, None, None, Some(&tag.id), None, 10).unwrap();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].id, b.id);

    // Query combined with a type filter.
    let hits = library::search_filtered(&conn, Some("borrowing"), Some("article"), None, None, 10)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, a.id);
    assert!(
        library::search_filtered(&conn, Some("borrowing"), Some("pdf"), None, None, 10)
            .unwrap()
            .is_empty()
    );

    // Per-type counts and distinct sources.
    let counts = library::type_counts(&conn).unwrap();
    assert!(counts
        .iter()
        .any(|(kind, count)| kind == "pdf" && *count == 1));
    assert!(counts
        .iter()
        .any(|(kind, count)| kind == "article" && *count == 1));
    assert_eq!(
        library::sources(&conn).unwrap(),
        vec!["Example".to_string(), "Kitchen".to_string()]
    );

    // URL lookup keeps source imports idempotent.
    assert_eq!(
        library::get_by_url(&conn, "https://example.com/rust")
            .unwrap()
            .map(|item| item.id),
        Some(a.id.clone())
    );
    assert!(library::get_by_url(&conn, "https://missing")
        .unwrap()
        .is_none());

    // Linking to a note, then unlinking.
    let note = el_db::repo::notes::create(
        &conn,
        &el_core::NewNote {
            title: "Note".to_string(),
            content: String::new(),
            project_id: None,
            folder_id: None,
            author: "user".to_string(),
            source_session_id: None,
            created_from_note_id: None,
        },
    )
    .unwrap();
    let linked = library::set_note(&conn, &a.id, Some(&note.id)).unwrap();
    assert_eq!(linked.note_id.as_deref(), Some(note.id.as_str()));
    assert!(library::set_note(&conn, &a.id, None)
        .unwrap()
        .note_id
        .is_none());

    // Publication date is patchable.
    let updated = library::update(
        &conn,
        &b.id,
        &el_core::UpdateLibraryItem {
            publication_date: Some("2020-01-01".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(updated.publication_date.as_deref(), Some("2020-01-01"));

    drop(conn);
    test.finish();
}

#[test]
fn skills_and_scripts_rename_in_place() {
    let test = TestDb::setup();
    let conn = test.conn();

    let skill = skills::save_skill(
        &conn,
        &NewSkill {
            name: "old".to_string(),
            description: "d".to_string(),
            content: "---\nname: old\n---\nbody".to_string(),
            parameters: "[]".to_string(),
            category: "general".to_string(),
        },
    )
    .unwrap();
    let renamed = skills::save_skill_with_rename(
        &conn,
        &NewSkill {
            name: "new".to_string(),
            description: "d2".to_string(),
            content: "---\nname: new\n---\nbody".to_string(),
            parameters: "[]".to_string(),
            category: "general".to_string(),
        },
        "old",
    )
    .unwrap();
    assert_eq!(renamed.id, skill.id);
    assert!(skills::get_skill_by_name(&conn, "old").unwrap().is_none());
    assert_eq!(skills::list_skills(&conn).unwrap().len(), 1);

    // Renaming onto a name that already exists is a conflict.
    skills::save_skill(
        &conn,
        &NewSkill {
            name: "taken".to_string(),
            description: String::new(),
            content: "body".to_string(),
            parameters: "[]".to_string(),
            category: "general".to_string(),
        },
    )
    .unwrap();
    assert!(skills::save_skill_with_rename(
        &conn,
        &NewSkill {
            name: "taken".to_string(),
            description: String::new(),
            content: "body".to_string(),
            parameters: "[]".to_string(),
            category: "general".to_string(),
        },
        "new",
    )
    .is_err());

    let script = skills::save_script(
        &conn,
        &NewScript {
            name: "sold".to_string(),
            description: String::new(),
            content: "print('x')".to_string(),
            parameters: "[]".to_string(),
            category: "general".to_string(),
            allow_network: false,
        },
    )
    .unwrap();
    let renamed = skills::save_script_with_rename(
        &conn,
        &NewScript {
            name: "snew".to_string(),
            description: String::new(),
            content: "print('y')".to_string(),
            parameters: "[]".to_string(),
            category: "general".to_string(),
            allow_network: true,
        },
        "sold",
    )
    .unwrap();
    assert_eq!(renamed.id, script.id);
    assert!(renamed.allow_network);
    assert!(skills::get_script_by_name(&conn, "sold").unwrap().is_none());
    assert_eq!(skills::list_scripts(&conn).unwrap().len(), 1);

    drop(conn);
    test.finish();
}

#[test]
fn skills_and_scripts_save_replace_and_delete() {
    let test = TestDb::setup();
    let conn = test.conn();

    let skill = skills::save_skill(
        &conn,
        &NewSkill {
            name: "summarize".to_string(),
            description: "Summarizes a note".to_string(),
            content: "Summarize: {{ text }}".to_string(),
            parameters: r#"{"text": "string"}"#.to_string(),
            category: "notes".to_string(),
        },
    )
    .unwrap();
    let replaced = skills::save_skill(
        &conn,
        &NewSkill {
            description: "Summarizes a note briefly".to_string(),
            ..NewSkill {
                name: "summarize".to_string(),
                description: String::new(),
                content: "Summarize briefly: {{ text }}".to_string(),
                parameters: "{}".to_string(),
                category: "notes".to_string(),
            }
        },
    )
    .unwrap();
    assert_eq!(replaced.id, skill.id);
    assert_eq!(skills::list_skills(&conn).unwrap().len(), 1);
    skills::delete_skill(&conn, "summarize").unwrap();
    assert!(skills::get_skill_by_name(&conn, "summarize")
        .unwrap()
        .is_none());

    let script = skills::save_script(
        &conn,
        &NewScript {
            name: "wordfreq".to_string(),
            description: "Word frequencies".to_string(),
            content: "print('hi')".to_string(),
            parameters: "{}".to_string(),
            category: "analysis".to_string(),
            allow_network: true,
        },
    )
    .unwrap();
    assert!(script.allow_network);
    assert_eq!(skills::list_scripts(&conn).unwrap().len(), 1);
    skills::delete_script(&conn, "wordfreq").unwrap();
    assert!(skills::get_script_by_name(&conn, "wordfreq")
        .unwrap()
        .is_none());

    drop(conn);
    test.finish();
}
