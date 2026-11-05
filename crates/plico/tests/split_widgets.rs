//! MyGO's split panes: the divider drags the first pane's size, updates the
//! application's value, and releases with the primary button.
use plico::prelude::*;
use plico::{Engine, Entity, WidgetRole};

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn named(engine: &Engine, label: &str) -> Entity {
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
    found.expect("a node with that label")
}

#[test]
fn split_drags_the_divider_and_reports_the_new_size() {
    let mut size = 200.0_f32;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        split(
            cx,
            &mut size,
            |cx| {
                text(cx, "First");
            },
            |cx| {
                text(cx, "Second");
            },
        );
    });
    assert!(engine.node(named(&engine, "First")).is_some());
    assert!(engine.node(named(&engine, "Second")).is_some());
    let divider = named(&engine, "Divider");
    assert_eq!(
        engine.node(divider).unwrap().style.role,
        Some(WidgetRole::Separator)
    );

    // Press anchors without moving; a move adds its delta to the size.
    engine.set_pressed(Some(divider));
    engine.set_pointer_position(Some((300., 10.)));
    let mut changed = false;
    frame(&mut engine, |cx| {
        changed = split(
            cx,
            &mut size,
            |cx| {
                text(cx, "First");
            },
            |cx| {
                text(cx, "Second");
            },
        );
    });
    assert_eq!(size, 200.0);
    assert!(!changed);
    engine.set_pointer_position(Some((350., 10.)));
    frame(&mut engine, |cx| {
        changed = split(
            cx,
            &mut size,
            |cx| {
                text(cx, "First");
            },
            |cx| {
                text(cx, "Second");
            },
        );
    });
    assert_eq!(size, 250.0);
    assert!(changed);

    // Release: moving the pointer no longer resizes.
    engine.set_pressed(None);
    engine.set_pointer_position(Some((500., 10.)));
    frame(&mut engine, |cx| {
        split(
            cx,
            &mut size,
            |cx| {
                text(cx, "First");
            },
            |cx| {
                text(cx, "Second");
            },
        );
    });
    assert_eq!(size, 250.0);
}

#[test]
fn split_clamps_the_first_pane_to_the_minimum() {
    let mut size = 60.0_f32;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        split(cx, &mut size, |_| {}, |_| {});
    });
    let divider = named(&engine, "Divider");
    engine.set_pressed(Some(divider));
    engine.set_pointer_position(Some((300., 10.)));
    frame(&mut engine, |cx| {
        split(cx, &mut size, |_| {}, |_| {});
    });
    engine.set_pointer_position(Some((-500., 10.)));
    frame(&mut engine, |cx| {
        split(cx, &mut size, |_| {}, |_| {});
    });
    assert_eq!(size, 40.0, "the theme's space(10) is the minimum pane");
}

#[test]
fn vertical_split_drags_the_height() {
    let mut size = 200.0_f32;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        split_vertical(cx, &mut size, |_| {}, |_| {});
    });
    let divider = named(&engine, "Divider");
    engine.set_pressed(Some(divider));
    engine.set_pointer_position(Some((10., 300.)));
    frame(&mut engine, |cx| {
        split_vertical(cx, &mut size, |_| {}, |_| {});
    });
    engine.set_pointer_position(Some((10., 350.)));
    frame(&mut engine, |cx| {
        split_vertical(cx, &mut size, |_| {}, |_| {});
    });
    assert_eq!(size, 250.0);
}

#[test]
fn percent_split_converts_the_drag_into_a_share() {
    let mut percent = 50.0_f32;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        split_percent(cx, &mut percent, |_| {}, |_| {});
    });
    let divider = named(&engine, "Divider");
    engine.set_pressed(Some(divider));
    engine.set_pointer_position(Some((300., 10.)));
    frame(&mut engine, |cx| {
        split_percent(cx, &mut percent, |_| {}, |_| {});
    });
    engine.set_pointer_position(Some((372., 10.)));
    frame(&mut engine, |cx| {
        split_percent(cx, &mut percent, |_| {}, |_| {});
    });
    // 719 px of room, so +72 px is about ten percent.
    assert!(
        (percent - 60.0).abs() < 1.0,
        "the drag becomes a share: {percent}"
    );
}
