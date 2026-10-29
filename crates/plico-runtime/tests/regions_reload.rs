use plico::prelude::*;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{
    Source,
    regions::{Checkpoint, OnClick, RegionApp, RegionProgram, Regions, State, WatchUi},
};
use std::{cell::RefCell, rc::Rc};

type Locals = Rc<RefCell<Vec<(State<i32>, State<i32>)>>>;
fn build(new_schema: bool, locals: Locals) -> (Regions, State<i32>, State<Vec<u64>>) {
    let mut saved = None;
    let app = Regions::new(|app| {
        let count = app.persisted_state(1, || 0_i32);
        let rows = app.persisted_state(2, || vec![10_u64, 20]);
        let version = app.state(|| new_schema);
        let locals = app.state(|| locals);
        saved = Some((count, rows));
        app.root(
            (count, rows, version, locals),
            |cx, (count, rows, version, locals)| {
                cx.watch((count.handle(),), |cx, (count,)| {
                    text!(cx, "Root {}", *count.read());
                });
                cx.watch_keyed(
                    1,
                    (rows.handle(), version.handle(), locals.handle()),
                    |cx, (rows, version, locals)| {
                        for &id in rows.read().iter() {
                            cx.component_keyed(
                                id,
                                (version.handle(), locals.handle()),
                                |cx, (version, locals)| {
                                    let kept = cx.local_persisted_state(1, || 0_i32);
                                    let transient = cx.local_state(|| 0_i32);
                                    locals.read().borrow_mut().push((kept, transient));
                                    if *version.read() {
                                        let changed = cx.local_persisted_state(2, || {
                                            String::from("new default")
                                        });
                                        cx.watch((changed,), |cx, (changed,)| {
                                            text(cx, &*changed.read());
                                        });
                                    } else {
                                        cx.local_persisted_state(2, || 42_i32);
                                    }
                                    cx.watch((kept, transient), |cx, (kept, transient)| {
                                        text!(cx, "Local {} / {}", *kept.read(), *transient.read());
                                    });
                                },
                            );
                        }
                    },
                );
            },
        );
    });
    let (count, rows) = saved.unwrap();
    (app, count, rows)
}
fn paint(s: &mut Source<Regions>) {
    s.render_retained_frame(&mut Scene::new(), &mut TextContext::new(), (700, 600), 1.)
        .unwrap();
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
}
#[test]
fn reload_restores_named_slots_resets_changed_types_and_recreates_owners() {
    let old_locals = Locals::default();
    let (app, count, rows) = build(false, Rc::clone(&old_locals));
    let mut old = Source::new(app);
    paint(&mut old);
    let (kept, transient) = old_locals.borrow()[0];
    old.view.access(count).set(8);
    old.view.access(kept).set(17);
    old.view.access(transient).set(91);
    old.view.access(rows).update(|r| r.reverse());
    paint(&mut old);
    let sender = old.view.sender(kept);
    let snapshot = plico_dev::StateSnapshot::encode(&old.view.checkpoint().unwrap()).unwrap();
    drop(old);
    assert_eq!(sender.send(99), Err(99));

    let new_locals = Locals::default();
    let (mut app, count, rows) = build(true, Rc::clone(&new_locals));
    app.restore_checkpoint(snapshot.decode::<Checkpoint>().unwrap());
    assert_eq!(*app.access(count).read(), 8);
    assert_eq!(*app.access(rows).read(), vec![20, 10]);
    assert!(app.try_access(kept).is_none());
    // Serializing again before local components mount must keep deferred data.
    assert_eq!(
        serde_json::to_value(app.checkpoint().unwrap()).unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let mut restored = Source::new(app);
    paint(&mut restored);
    let values: Vec<_> = new_locals
        .borrow()
        .iter()
        .map(|&(kept, transient)| {
            (
                *restored.view.access(kept).read(),
                *restored.view.access(transient).read(),
            )
        })
        .collect();
    assert_eq!(
        values,
        vec![(0, 0), (17, 0)],
        "business keys survive reordering; unmarked values reset"
    );
    assert_eq!(
        restored.view.restore_stats(),
        plico_runtime::regions::RestoreStats {
            restored: 4,
            reset: 2,
            unmatched: 0
        }
    );
    assert!(
        restored
            .adapter
            .placed
            .iter()
            .any(|p| p.paint.text.as_deref() == Some("new default"))
    );
}

#[test]
fn checkpoint_handles_non_json_map_keys_and_new_or_removed_slots() {
    use std::collections::HashMap;
    let mut handle = None;
    let old = Regions::new(|app| {
        let map = app.persisted_state(1, || {
            HashMap::from([((1_i32, 2_i32), String::from("value"))])
        });
        app.persisted_state(2, || 123_i32);
        handle = Some(map);
        app.root((), |_, ()| {});
    });
    let bytes = plico_dev::StateSnapshot::encode(&old.checkpoint().unwrap()).unwrap();
    let mut new_handle = None;
    let mut app = Regions::new(|app| {
        let map = app.persisted_state(1, HashMap::<(i32, i32), String>::new);
        app.persisted_state(3, || String::from("added"));
        new_handle = Some(map);
        app.root((), |_, ()| {});
    });
    app.restore_checkpoint(bytes.decode().unwrap());
    assert_eq!(
        app.access(new_handle.unwrap()).read().get(&(1, 2)).unwrap(),
        "value"
    );
    assert!(app.try_access(handle.unwrap()).is_none());
    let mut source = Source::new(app);
    paint(&mut source);
    assert_eq!(
        source.view.restore_stats(),
        plico_runtime::regions::RestoreStats {
            restored: 1,
            reset: 0,
            unmatched: 1
        }
    );
}

struct Program;
impl RegionProgram for Program {
    fn build() -> Regions {
        Regions::new(|app| {
            let count = app.persisted_state(1, || 0_i32);
            app.root((count,), |cx, (count,)| {
                cx.watch_keyed(1, (count.handle(),), |cx, (count,)| {
                    text!(cx, "Count {}", *count.read());
                    button(cx).key("increment").text("Increment").on_click(
                        (count.handle(),),
                        |(count,)| {
                            count.update(|v| *v += 1);
                        },
                    );
                });
            });
        })
    }
}
fn find_path(nodes: &[plico::ViewNode], label: &str) -> Option<Vec<plico::PathKey>> {
    for node in nodes {
        if node.text == label {
            return Some(vec![node.key.clone()]);
        }
        if let Some(mut path) = find_path(&node.children, label) {
            path.insert(0, node.key.clone());
            return Some(path);
        }
    }
    None
}
fn has_text(nodes: &[plico::ViewNode], text: &str) -> bool {
    nodes
        .iter()
        .any(|n| n.text == text || has_text(&n.children, text))
}
#[test]
fn existing_dev_protocol_rebuilds_functions_and_restores_region_checkpoint() {
    use plico_dev::{Request, render};
    type App = RegionApp<Program>;
    let frame = render::<App>(Request::default()).unwrap();
    let path = find_path(&frame.nodes, "Increment").unwrap();
    let clicked = render::<App>(Request {
        resume: true,
        clicks: vec![path],
        ..Default::default()
    })
    .unwrap();
    assert!(has_text(&clicked.nodes, "Count 1"));
    // resume=false explicitly recreates the session, as a new image would.
    let restored = render::<App>(Request {
        state: clicked.state,
        ..Default::default()
    })
    .unwrap();
    assert!(restored.reset.is_none(), "{:?}", restored.reset);
    assert!(has_text(&restored.nodes, "Count 1"));
    let next = render::<App>(Request {
        resume: true,
        clicks: vec![find_path(&restored.nodes, "Increment").unwrap()],
        ..Default::default()
    })
    .unwrap();
    assert!(has_text(&next.nodes, "Count 2"));
}

#[test]
fn corrupt_slot_payload_resets_only_that_slot() {
    let old = Regions::new(|app| {
        app.persisted_state(1, || 11_i32);
        app.persisted_state(2, || 22_i32);
        app.root((), |_, ()| {});
    });
    let mut data = serde_json::to_value(old.checkpoint().unwrap()).unwrap();
    let record = data["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["key"]["key"] == 1)
        .unwrap();
    record["value"] = serde_json::json!([0xc1]); // reserved/invalid MessagePack byte
    let mut handles = None;
    let mut app = Regions::new(|app| {
        let a = app.persisted_state(1, || -1_i32);
        let b = app.persisted_state(2, || -2_i32);
        handles = Some((a, b));
        app.root((), |_, ()| {});
    });
    app.restore_checkpoint(serde_json::from_value(data).unwrap());
    let (a, b) = handles.unwrap();
    assert_eq!(*app.access(a).read(), -1);
    assert_eq!(*app.access(b).read(), 22);
    assert_eq!(app.restore_stats().reset, 1);
    assert_eq!(app.restore_stats().restored, 1);
}

#[test]
fn region_app_forwards_root_theme_through_development_protocol() {
    struct Themed;
    impl RegionProgram for Themed {
        fn build() -> Regions {
            Regions::new(|app| {
                let theme = app.persisted_state(1, plico::Theme::dark);
                app.theme_state(theme);
                app.root((), |cx, ()| {
                    text(cx, "Themed");
                });
            })
        }
    }
    let frame = plico_dev::render::<RegionApp<Themed>>(plico_dev::Request::default()).unwrap();
    assert_eq!(frame.theme, plico::Theme::dark());
}

#[test]
fn development_reload_rebuilds_property_callbacks_and_restores_dynamic_attributes() {
    use plico_dev::{Request, render};
    use plico_runtime::regions::Reactive;
    struct Styled;
    impl RegionProgram for Styled {
        fn build() -> Regions {
            Regions::new(|app| {
                let size = app.persisted_state(1, || 18_u16);
                app.root((size,), |cx, (size,)| {
                    text_empty(cx).reactive((size.handle(),), |node, (size,)| {
                        let size = *size.read();
                        node.text(format!("Size {size}"))
                            .font_size(size)
                            .weight(if size > 18 { 700 } else { 400 })
                            .opacity(0.65);
                    });
                    button(cx)
                        .text("Bigger")
                        .on_click((size.handle(),), |(size,)| size.update(|size| *size += 4));
                });
            })
        }
    }
    fn find<'a>(nodes: &'a [plico::ViewNode], label: &str) -> Option<&'a plico::ViewNode> {
        nodes.iter().find_map(|node| {
            if node.text == label {
                Some(node)
            } else {
                find(&node.children, label)
            }
        })
    }
    type App = RegionApp<Styled>;
    let first = render::<App>(Request::default()).unwrap();
    let clicked = render::<App>(Request {
        resume: true,
        clicks: vec![find_path(&first.nodes, "Bigger").unwrap()],
        ..Default::default()
    })
    .unwrap();
    assert!(has_text(&clicked.nodes, "Size 22"));
    let restored = render::<App>(Request {
        state: clicked.state,
        ..Default::default()
    })
    .unwrap();
    assert!(restored.reset.is_none());
    let style = find(&restored.nodes, "Size 22").unwrap().style;
    assert_eq!(
        (style.font_size, style.font_weight, style.opacity),
        (22, 700, 0.65)
    );
    let next = render::<App>(Request {
        resume: true,
        clicks: vec![find_path(&restored.nodes, "Bigger").unwrap()],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(find(&next.nodes, "Size 26").unwrap().style.font_size, 26);
}
