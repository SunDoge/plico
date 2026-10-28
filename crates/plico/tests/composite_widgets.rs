use plico::{Engine, Entity, prelude::*};

fn named(engine: &Engine, label: &str) -> Entity {
    fn walk(engine: &Engine, id: Entity, label: &str) -> Option<Entity> {
        let node = engine.node(id).unwrap();
        if node.text == label {
            return Some(id);
        }
        node.children.iter().find_map(|id| walk(engine, *id, label))
    }
    walk(engine, engine.root(), label).unwrap()
}
#[component]
fn screen(cx: &mut UiContext, selected: &mut usize, open: &mut bool, disabled: bool) {
    column(cx).disabled(disabled).children(|cx| {
        segmented(cx, selected, &["Stable", "Beta"]);
        collapsible(cx, "Details", open, |cx| {
            text(cx, "Panel content");
        });
        badge(cx, "Ready");
        separator(cx);
    })
}
#[test]
fn selection_disclosure_identity_and_disabled_state() {
    let mut e = Engine::default();
    let (mut selected, mut open) = (0, false);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    let beta = named(&e, "Beta");
    let details = named(&e, "Details");
    assert_eq!(e.node(details).unwrap().style.expanded, Some(false));
    e.queue_click(beta);
    e.queue_click(details);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    assert_eq!(selected, 1);
    assert!(open);
    let content = named(&e, "Panel content");
    // A settling declaration updates selected/expanded semantic states.
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    assert_eq!(named(&e, "Beta"), beta);
    assert_eq!(e.node(beta).unwrap().style.selected, Some(true));
    assert_eq!(e.node(details).unwrap().style.expanded, Some(true));
    e.queue_click(named(&e, "Stable"));
    e.queue_click(details);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, true))
        .unwrap();
    assert_eq!(selected, 1);
    assert!(open);
    e.queue_click(details);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    assert!(!open);
    assert!(e.node(content).is_none());
}
#[test]
fn empty_selection_and_invalid_progress_values_are_safe() {
    let mut e = Engine::default();
    let mut selected = 99;
    e.frame_declarations(|cx| {
        assert!(!segmented(cx, &mut selected, &[]));
        for (index, value) in [f32::NAN, f32::INFINITY, -2., 2.].into_iter().enumerate() {
            cx.keyed(index as u64, |cx| progress(cx, "Download", value));
        }
    })
    .unwrap();
    assert_eq!(selected, 99);
    named(&e, "0%");
    named(&e, "100%");
    fn fills(e: &Engine, id: Entity, widths: &mut Vec<f32>) {
        let n = e.node(id).unwrap();
        if n.style.background == Some(Color::Accent) {
            widths.push(n.style.width_percent.unwrap());
        }
        for child in n.children {
            fills(e, *child, widths);
        }
    }
    let mut widths = Vec::new();
    fills(&e, e.root(), &mut widths);
    assert_eq!(widths, [0., 0., 0., 100.]);
}

#[component]
fn panel_screen(cx: &mut UiContext, picked: &mut bool) {
    column(cx).children(|cx| {
        panel(cx, "Network").children(|cx| {
            text(cx, "Panel content");
            if button(cx).text("Pick").clicked() {
                *picked = true;
            }
        });
        panel(cx, None).padding(2).children(|cx| {
            text(cx, "Untitled content");
        });
        stat(cx, "Throughput", "1.2 GiB/s", Some("over 30 seconds"));
        stat(cx, "Errors", "0", None);
        let _ = nav_item(cx, "Overview", true, None);
        let _ = nav_item(cx, "Activity", false, None);
        let _ = *picked;
    })
}
#[test]
fn panel_stat_and_nav_item_mount_with_expected_structure() {
    let mut e = Engine::default();
    let mut picked = false;
    e.frame_declarations(|cx| panel_screen(cx, &mut picked))
        .unwrap();
    // The titled panel declares its heading above its content: both texts
    // share the panel column as parent, in declaration order.
    fn parent_texts(engine: &Engine, child: Entity) -> (Entity, Vec<String>) {
        fn find_parent(engine: &Engine, id: Entity, target: Entity) -> Option<Entity> {
            let node = engine.node(id).unwrap();
            if node.children.contains(&target) {
                return Some(id);
            }
            node.children
                .iter()
                .find_map(|&c| find_parent(engine, c, target))
        }
        let parent = find_parent(engine, engine.root(), child).unwrap();
        let texts = engine
            .node(parent)
            .unwrap()
            .children
            .iter()
            .filter_map(|&c| engine.node(c).map(|n| n.text.to_string()))
            .collect();
        (parent, texts)
    }
    // Button text is stored on the button node itself, so the panel's direct
    // text children come first in declaration order.
    let (_, heading_texts) = parent_texts(&e, named(&e, "Network"));
    assert!(heading_texts.starts_with(&["Network".to_string(), "Panel content".to_string()]));
    let (_, untitled_texts) = parent_texts(&e, named(&e, "Untitled content"));
    assert_eq!(untitled_texts, vec!["Untitled content".to_string()]);
    // stat declares label, value, caption in order.
    let (_, stat_texts) = parent_texts(&e, named(&e, "1.2 GiB/s"));
    assert_eq!(
        stat_texts,
        vec![
            "Throughput".to_string(),
            "1.2 GiB/s".to_string(),
            "over 30 seconds".to_string()
        ]
    );
    // The captionless stat has exactly two children.
    let (_, error_texts) = parent_texts(&e, named(&e, "Errors"));
    assert_eq!(error_texts, vec!["Errors".to_string(), "0".to_string()]);
    // nav_item selected state colors differ from unselected.
    fn styled(engine: &Engine, label: &str) -> plico::Style {
        engine.node(named(engine, label)).unwrap().style
    }
    assert_eq!(
        styled(&e, "Overview").background,
        Some(plico::Color::Selection)
    );
    assert_eq!(styled(&e, "Activity").background, Some(plico::Color::Panel));
}
