//! MyGO's breadcrumb path: links for the parents, plain text for the current
//! item, and the navigation landmark roles.
use plico::prelude::*;
use plico::{Engine, Entity, Kind, WidgetRole};

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn walk(engine: &Engine, id: Entity, visit: &mut impl FnMut(Entity, plico::NodeRef<'_>)) {
    let node = engine.node(id).unwrap();
    let children = node.children.to_vec();
    visit(id, node);
    for child in children {
        walk(engine, child, visit);
    }
}

fn named(engine: &Engine, label: &str) -> Entity {
    let mut found = None;
    walk(engine, engine.root(), &mut |entity, node| {
        if node.text == label && found.is_none() {
            found = Some(entity);
        }
    });
    found.unwrap_or_else(|| panic!("no node named {label:?}"))
}

#[test]
fn breadcrumbs_link_the_parent_items_and_name_the_current_one() {
    let mut chosen = 3;
    let mut engine = Engine::default();
    let mut changed = false;
    frame(&mut engine, |cx| {
        changed = breadcrumbs(cx, &["Home", "Users", "ada", "Documents"], &mut chosen);
    });
    assert!(!changed);
    assert_eq!(
        engine.node(named(&engine, "Documents")).unwrap().kind,
        Kind::Text,
        "the last item is where the user is, not a link"
    );
    let mut navigation = 0;
    let mut links = 0;
    walk(
        &engine,
        engine.root(),
        &mut |_, node| match node.style.role {
            Some(WidgetRole::Navigation) => navigation += 1,
            Some(WidgetRole::Link) => links += 1,
            _ => {}
        },
    );
    assert_eq!(navigation, 1, "the row is one navigation landmark");
    assert_eq!(links, 3, "every item but the last is a link");

    engine.queue_click(named(&engine, "Users"));
    frame(&mut engine, |cx| {
        changed = breadcrumbs(cx, &["Home", "Users", "ada", "Documents"], &mut chosen);
    });
    assert_eq!(chosen, 1);
    assert!(changed, "clicking a parent reports the new choice");
}

#[test]
fn breadcrumbs_clamp_an_out_of_range_choice_to_the_path() {
    let mut chosen = 99;
    let mut engine = Engine::default();
    let mut changed = false;
    frame(&mut engine, |cx| {
        changed = breadcrumbs(cx, &["Home", "Users", "Documents"], &mut chosen);
    });
    assert_eq!(chosen, 2);
    assert!(changed, "clamping to the last item is a reported change");
}

#[test]
fn breadcrumbs_leave_an_empty_path_alone() {
    let mut chosen = 2;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        assert!(!breadcrumbs(cx, &[], &mut chosen));
    });
    assert_eq!(chosen, 2);
}

fn optional(engine: &Engine, label: &str) -> Option<Entity> {
    let mut found = None;
    walk(engine, engine.root(), &mut |entity, node| {
        if node.text == label && found.is_none() {
            found = Some(entity);
        }
    });
    found
}

#[test]
fn sidebar_sections_toggle_and_items_report_their_choice() {
    let mut open = true;
    let mut chosen = "inbox".to_string();
    let mut engine = Engine::default();
    let section = |cx: &mut UiContext, chosen: &mut String, open: &mut bool| {
        let _ = sidebar(cx).width(220.).height(320.)
            + |cx| {
                sidebar_section(cx, "Mailboxes", Some(open), |cx| {
                    if sidebar_item(cx, "Inbox", chosen == "inbox", None).clicked() {
                        *chosen = "inbox".into();
                    }
                    if sidebar_item(cx, "Sent", chosen == "sent", None).clicked() {
                        *chosen = "sent".into();
                    }
                });
            };
    };
    frame(&mut engine, |cx| {
        section(cx, &mut chosen, &mut open);
    });
    let inbox = named(&engine, "Inbox");
    assert_eq!(engine.node(inbox).unwrap().style.selected, Some(true));
    assert_eq!(
        engine.node(inbox).unwrap().style.role,
        Some(WidgetRole::TreeItem)
    );
    let mut trees = 0;
    walk(&engine, engine.root(), &mut |_, node| {
        if node.style.role == Some(WidgetRole::Tree) {
            trees += 1;
        }
    });
    assert_eq!(trees, 1, "the sidebar is the tree landmark");

    engine.queue_click(named(&engine, "Sent"));
    frame(&mut engine, |cx| {
        section(cx, &mut chosen, &mut open);
    });
    assert_eq!(chosen, "sent");

    engine.queue_click(named(&engine, "Mailboxes"));
    frame(&mut engine, |cx| {
        section(cx, &mut chosen, &mut open);
    });
    assert!(!open, "the section title toggles the disclosure");
    assert!(
        optional(&engine, "Inbox").is_none(),
        "closed sections unmount their items"
    );
    // The toggle lands through clicked(), evaluated after the header is
    // declared, so the expanded state appears on the next declaration.
    frame(&mut engine, |cx| {
        section(cx, &mut chosen, &mut open);
    });
    let header = engine.node(named(&engine, "Mailboxes")).unwrap();
    assert_eq!(header.style.expanded, Some(false));
}
