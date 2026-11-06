//! MyGO's tree items: the disclosure arrow toggles application state, the
//! label reports selection, and closed branches unmount their children.
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
fn tree_items_report_selection_and_their_roles() {
    let mut open = true;
    let mut chosen = "src".to_string();
    let mut engine = Engine::default();
    let tree_view = |cx: &mut UiContext, chosen: &mut String, open: &mut bool| {
        tree(cx, |cx| {
            let branch_selected = chosen == "src";
            let branch = tree_item(cx, "src", branch_selected, Some(open), 0, |cx| {
                let leaf_selected = chosen == "main.go";
                if tree_item(cx, "main.go", leaf_selected, None, 1, |_| {}).clicked() {
                    *chosen = "main.go".into();
                }
            });
            if branch.clicked() {
                *chosen = "src".into();
            }
            if tree_item(cx, "go.mod", chosen == "go.mod", None, 0, |_| {}).clicked() {
                *chosen = "go.mod".into();
            }
        });
    };
    frame(&mut engine, |cx| tree_view(cx, &mut chosen, &mut open));
    assert_eq!(
        engine.node(named(&engine, "main.go")).unwrap().style.role,
        Some(WidgetRole::TreeItem)
    );
    assert_eq!(
        engine.node(named(&engine, "src")).unwrap().style.expanded,
        Some(true)
    );
    assert_eq!(
        engine
            .node(named(&engine, "go.mod"))
            .unwrap()
            .style
            .expanded,
        None,
        "a leaf has no expanded state"
    );
    let mut trees = 0;
    fn count_trees(engine: &Engine, id: Entity, trees: &mut usize) {
        let node = engine.node(id).unwrap();
        if node.style.role == Some(WidgetRole::Tree) {
            *trees += 1;
        }
        let children = node.children.to_vec();
        for child in children {
            count_trees(engine, child, trees);
        }
    }
    count_trees(&engine, engine.root(), &mut trees);
    assert_eq!(trees, 1, "the root column is the tree");

    engine.queue_click(named(&engine, "main.go"));
    frame(&mut engine, |cx| tree_view(cx, &mut chosen, &mut open));
    assert_eq!(chosen, "main.go");
    // The selected flag was read before the click landed; the next
    // declaration paints the choice.
    frame(&mut engine, |cx| tree_view(cx, &mut chosen, &mut open));
    assert_eq!(
        engine
            .node(named(&engine, "main.go"))
            .unwrap()
            .style
            .selected,
        Some(true)
    );
}

#[test]
fn collapsing_a_branch_unmounts_its_items() {
    let mut open = true;
    let mut engine = Engine::default();
    let tree_view = |cx: &mut UiContext, open: &mut bool| {
        tree(cx, |cx| {
            let _ = tree_item(cx, "src", false, Some(open), 0, |cx| {
                let _ = tree_item(cx, "main.go", false, None, 1, |_| {});
            });
        });
    };
    frame(&mut engine, |cx| tree_view(cx, &mut open));
    assert!(find(&engine, "main.go").is_some());
    engine.queue_click(named(&engine, "Collapse"));
    frame(&mut engine, |cx| tree_view(cx, &mut open));
    assert!(!open);
    assert!(
        find(&engine, "main.go").is_none(),
        "a closed branch unmounts its children"
    );
    // The arrow's name and expanded state were declared before the click
    // landed; the next declaration shows the closed branch.
    frame(&mut engine, |cx| tree_view(cx, &mut open));
    assert!(find(&engine, "Expand").is_some());
    assert_eq!(
        engine.node(named(&engine, "src")).unwrap().style.expanded,
        Some(false)
    );
}
