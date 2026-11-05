//! MyGO's radio buttons, text link and router history buttons.
use plico::prelude::*;
use plico::{Engine, Entity, WidgetRole};

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn find(engine: &Engine, label: &str) -> Option<Entity> {
    fn walk(engine: &Engine, id: Entity, label: &str, found: &mut Option<Entity>) {
        let node = engine.node(id).unwrap();
        if node.text == label && found.is_none() {
            *found = Some(id);
        }
        let children = node.children.to_vec();
        for child in children {
            walk(engine, child, label, found);
        }
    }
    let mut found = None;
    walk(engine, engine.root(), label, &mut found);
    found
}

fn named(engine: &Engine, label: &str) -> Entity {
    find(engine, label).unwrap_or_else(|| panic!("no node named {label:?}"))
}

#[test]
fn radios_select_the_caller_value_and_expose_their_roles() {
    let mut choice = "Medium";
    let mut engine = Engine::default();
    let radio_view = |cx: &mut UiContext, choice: &mut &'static str| {
        radio_group(cx, |cx| {
            for size in ["Small", "Medium", "Large"] {
                if radio(cx, size, *choice == size).clicked() {
                    *choice = size;
                }
            }
        });
    };
    frame(&mut engine, |cx| radio_view(cx, &mut choice));
    assert_eq!(
        engine
            .node(named(&engine, "Medium"))
            .unwrap()
            .style
            .selected,
        Some(true)
    );
    assert_eq!(
        engine.node(named(&engine, "Medium")).unwrap().style.role,
        Some(WidgetRole::RadioButton)
    );
    let mut groups = 0;
    fn count_role(engine: &Engine, id: Entity, role: WidgetRole, count: &mut usize) {
        let node = engine.node(id).unwrap();
        if node.style.role == Some(role) {
            *count += 1;
        }
        let children = node.children.to_vec();
        for child in children {
            count_role(engine, child, role, count);
        }
    }
    count_role(&engine, engine.root(), WidgetRole::RadioGroup, &mut groups);
    assert_eq!(groups, 1);

    engine.queue_click(named(&engine, "Large"));
    frame(&mut engine, |cx| radio_view(cx, &mut choice));
    assert_eq!(choice, "Large");
    frame(&mut engine, |cx| radio_view(cx, &mut choice));
    assert_eq!(
        engine.node(named(&engine, "Large")).unwrap().style.selected,
        Some(true)
    );
    assert_eq!(
        engine
            .node(named(&engine, "Medium"))
            .unwrap()
            .style
            .selected,
        Some(false)
    );
}

#[test]
fn link_is_accent_colored_and_reports_clicks() {
    let mut engine = Engine::default();
    let mut clicked = false;
    frame(&mut engine, |cx| {
        clicked = link(cx, "Privacy policy").clicked();
    });
    assert!(!clicked);
    let node = engine.node(named(&engine, "Privacy policy")).unwrap();
    assert_eq!(node.style.role, Some(WidgetRole::Link));
    assert_eq!(node.style.foreground, Color::Accent);
    engine.queue_click(named(&engine, "Privacy policy"));
    frame(&mut engine, |cx| {
        clicked = link(cx, "Privacy policy").clicked();
    });
    assert!(clicked, "the declaration site decides what the link does");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Route)]
enum Page {
    Home,
    Settings,
}

#[test]
fn history_buttons_follow_the_router_and_disable_at_the_ends() {
    let mut router = Router::new(Page::Home);
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        back_button(cx, &mut router);
        forward_button(cx, &mut router);
    });
    assert!(engine.node(named(&engine, "Back")).unwrap().style.disabled);
    assert!(
        engine
            .node(named(&engine, "Forward"))
            .unwrap()
            .style
            .disabled
    );

    router.go(Page::Settings);
    frame(&mut engine, |cx| {
        back_button(cx, &mut router);
        forward_button(cx, &mut router);
    });
    assert!(!engine.node(named(&engine, "Back")).unwrap().style.disabled);
    assert!(
        engine
            .node(named(&engine, "Forward"))
            .unwrap()
            .style
            .disabled
    );

    engine.queue_click(named(&engine, "Back"));
    frame(&mut engine, |cx| {
        back_button(cx, &mut router);
        forward_button(cx, &mut router);
    });
    assert_eq!(router.current(), Page::Home);
    frame(&mut engine, |cx| {
        back_button(cx, &mut router);
        forward_button(cx, &mut router);
    });
    assert!(engine.node(named(&engine, "Back")).unwrap().style.disabled);
    assert!(
        !engine
            .node(named(&engine, "Forward"))
            .unwrap()
            .style
            .disabled
    );

    engine.queue_click(named(&engine, "Forward"));
    frame(&mut engine, |cx| {
        back_button(cx, &mut router);
        forward_button(cx, &mut router);
    });
    assert_eq!(router.current(), Page::Settings);
}
