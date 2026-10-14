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
