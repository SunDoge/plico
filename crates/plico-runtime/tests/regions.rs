#![cfg(feature = "regions")]
use plico::prelude::*;
use plico_renderer::{FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
use plico_runtime::{
    Source,
    regions::{OnClick, Regions, State, WatchUi},
};

fn app() -> (Regions, State<i32>) {
    let mut saved = None;
    let app = Regions::new(|app| {
        let count = app.state(|| 0_i32);
        saved = Some(count);
        let expanded = app.state(|| true);
        app.root((count, expanded), |cx, (count, expanded)| {
            column(cx).key("shell").gap(8).children(|cx| {
                text(cx, "Static header").key("header");
                row(cx).key("buttons").gap(8).children(|cx| {
                    button(cx)
                        .text("Increment")
                        .on_click((count.handle(),), |(count,)| {
                            count.update(|v| *v += 1);
                        });
                    button(cx)
                        .text("Toggle")
                        .on_click((expanded.handle(),), |(expanded,)| {
                            expanded.update(|v| *v = !*v);
                        });
                });
                cx.watch_keyed(1, (count.handle(),), |cx, (count,)| {
                    text!(cx, "Count {}", *count.read());
                });
                cx.watch_keyed(
                    2,
                    (expanded.handle(), count.handle()),
                    |cx, (expanded, count)| {
                        if *expanded.read() {
                            row(cx).gap(4).children(|cx| {
                                text(cx, "Details");
                                cx.watch_keyed(3, (count.handle(),), |cx, (count,)| {
                                    text!(cx, "Double {}", *count.read() * 2);
                                    button(cx).text("Inner increment").on_click(
                                        (count.handle(),),
                                        |(count,)| {
                                            count.update(|v| *v += 1);
                                        },
                                    );
                                });
                            });
                        }
                    },
                );
            });
        });
    });
    (app, saved.unwrap())
}
fn paint(source: &mut Source<Regions>, text: &mut TextContext, size: (u32, u32)) {
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, text, size, 1.)
        .unwrap();
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn contains(source: &Source<Regions>, label: &str) -> bool {
    source
        .adapter
        .placed
        .iter()
        .any(|p| p.paint.text.as_deref() == Some(label))
}
fn click(source: &mut Source<Regions>, label: &str) {
    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, label, None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target }));
}
fn root_runs(source: &Source<Regions>) -> usize {
    let entity = source
        .engine
        .find(source.engine.root(), "regions-root")
        .unwrap();
    source
        .view
        .region_runs()
        .into_iter()
        .find(|(e, _)| *e == entity)
        .unwrap()
        .1
}
#[test]
fn real_button_event_updates_native_watch_and_nested_widgets() {
    let (app, count) = app();
    let mut source = Source::new(app);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    click(&mut source, "Increment");
    paint(&mut source, &mut text, (600, 400));
    assert_eq!(*source.view.access(count).read(), 1);
    assert!(contains(&source, "Count 1"));
    assert!(contains(&source, "Double 2"));
    click(&mut source, "Inner increment");
    paint(&mut source, &mut text, (600, 400));
    assert_eq!(*source.view.access(count).read(), 2);
    assert!(contains(&source, "Count 2"));
    assert!(contains(&source, "Double 4"));
}
#[test]
fn external_state_change_skips_component_root() {
    let (app, count) = app();
    let mut source = Source::new(app);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    let calls = root_runs(&source);
    source.view.access(count).update(|v| *v = 8);
    paint(&mut source, &mut text, (600, 400));
    assert_eq!(root_runs(&source), calls);
    assert!(contains(&source, "Count 8"));
    assert!(contains(&source, "Double 16"));
}
#[test]
fn branch_removal_rejects_old_native_events_and_preserves_component_state() {
    let (app, count) = app();
    let mut source = Source::new(app);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    let old = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Inner increment", None)
        .unwrap()
        .id;
    source.view.access(count).update(|v| *v = 7);
    click(&mut source, "Toggle");
    paint(&mut source, &mut text, (600, 400));
    assert!(!contains(&source, "Details"));
    assert!(!source.handle_semantic_action(SemanticAction::Click { target: old }));
    click(&mut source, "Toggle");
    paint(&mut source, &mut text, (600, 400));
    assert!(contains(&source, "Double 14"));
    assert_eq!(*source.view.access(count).read(), 7);
}
#[test]
fn resize_full_declaration_refreshes_contexts_and_next_update_stays_local() {
    let (app, count) = app();
    let mut source = Source::new(app);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    source.view.access(count).update(|v| *v = 5);
    paint(&mut source, &mut text, (600, 400));
    paint(&mut source, &mut text, (800, 500));
    let calls = root_runs(&source);
    assert!(contains(&source, "Count 5"));
    source.view.access(count).update(|v| *v = 6);
    paint(&mut source, &mut text, (800, 500));
    assert_eq!(root_runs(&source), calls);
    assert!(contains(&source, "Count 6"));
}
#[test]
fn repeated_toggle_does_not_leak_engine_or_region_nodes() {
    let (app, _) = app();
    let mut source = Source::new(app);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    let nodes = source.experiment_live_counts();
    let regions = source.view.region_runs().len();
    for _ in 0..40 {
        click(&mut source, "Toggle");
        paint(&mut source, &mut text, (600, 400));
        click(&mut source, "Toggle");
        paint(&mut source, &mut text, (600, 400));
        assert_eq!(source.experiment_live_counts(), nodes);
        assert_eq!(source.view.region_runs().len(), regions);
    }
}

#[test]
fn deep_regions_are_not_mistaken_for_an_eight_pass_state_cycle() {
    struct Level {
        value: i32,
        next: Option<State<Level>>,
    }
    fn level(cx: &mut UiContext, (data,): (plico_runtime::regions::Access<'_, Level>,)) {
        text!(cx, "Level {}", data.read().value);
        if let Some(next) = data.read().next {
            cx.watch_keyed(1, (next,), level);
        }
    }
    let mut saved = None;
    let app = Regions::new(|app| {
        let mut next = None;
        for value in (0..12).rev() {
            next = Some(app.state(|| Level { value, next }));
        }
        let top = next.unwrap();
        saved = Some(top);
        app.root((top,), |cx, (top,)| {
            cx.watch_keyed(1, (top.handle(),), level)
        });
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 800));
    source.view.access(saved.unwrap()).update(|v| v.value = 99);
    paint(&mut source, &mut text, (600, 800));
    assert!(contains(&source, "Level 99"));
}

#[test]
fn owner_drop_releases_state_without_handles_keeping_it_alive() {
    use std::{cell::Cell, rc::Rc};
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut saved = None;
    let app = Regions::new(|app| {
        saved = Some(app.state(|| Probe(Rc::clone(&drops))));
        app.root((), |cx, ()| {
            text(cx, "root");
        });
    });
    assert_eq!(drops.get(), 0);
    drop(app);
    assert_eq!(drops.get(), 1);
    let _still_a_copy = saved.unwrap();
    assert_eq!(drops.get(), 1);
}

#[test]
fn heterogeneous_tuple_arguments_are_inferred_for_watch_and_event() {
    let app = Regions::new(|app| {
        let a = app.state(|| 0_i32);
        let b = app.state(|| String::from("x"));
        let c = app.state(|| false);
        app.root((a, b, c), |cx, (a, b, c)| {
            button(cx)
                .text("Three")
                .on_click((a.handle(), b.handle(), c.handle()), |(a, b, c)| {
                    a.update(|v| *v += 1);
                    b.update(|v| v.push('!'));
                    c.update(|v| *v = !*v);
                });
            cx.watch((a.handle(), b.handle(), c.handle()), |cx, (a, b, c)| {
                text!(cx, "{} {} {}", *a.read(), *b.read(), *c.read());
            });
        });
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    click(&mut source, "Three");
    paint(&mut source, &mut text, (600, 400));
    assert!(contains(&source, "1 x! true"));
}

#[component]
fn scoped_component(cx: &mut UiContext, (count,): (plico_runtime::regions::Access<'_, i32>,)) {
    column(cx).children(|cx| {
        cx.watch((count.handle(),), |cx, (count,)| {
            text!(cx, "Scoped {}", *count.read());
            button(cx)
                .text("Scoped increment")
                .on_click((count.handle(),), |(count,)| {
                    count.update(|v| *v += 1);
                });
        });
    });
}
#[test]
fn component_macro_scopes_survive_native_region_replay() {
    let app = Regions::new(|app| {
        let count = app.state(|| 0_i32);
        app.root((count,), scoped_component);
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    let calls = root_runs(&source);
    click(&mut source, "Scoped increment");
    paint(&mut source, &mut text, (600, 400));
    assert!(contains(&source, "Scoped 1"));
    assert_eq!(root_runs(&source), calls);
}

#[test]
fn component_local_state_survives_reorder_and_dies_on_unmount() {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    struct Local {
        count: i32,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for Local {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    type Records = Rc<RefCell<Vec<State<Local>>>>;
    let drops = Rc::new(Cell::new(0));
    let records: Records = Rc::default();
    let mut ids = None;
    let app = Regions::new(|app| {
        let rows = app.state(|| vec![1_u64, 2]);
        ids = Some(rows);
        let probe = app.state(|| (Rc::clone(&drops), Rc::clone(&records)));
        app.root((rows, probe), |cx, (rows, probe)| {
            cx.watch((rows.handle(), probe.handle()), |cx, (rows, probe)| {
                for &key in rows.read().iter() {
                    cx.component_keyed(key, (probe.handle(),), |cx, (probe,)| {
                        let local = cx.local_state(|| Local {
                            count: 0,
                            drops: Rc::clone(&probe.read().0),
                        });
                        probe.read().1.borrow_mut().push(local);
                        cx.watch((local,), |cx, (local,)| {
                            text!(cx, "Local {}", local.read().count);
                            button(cx).text("Local increment").on_click(
                                (local.handle(),),
                                |(local,)| {
                                    local.update(|v| v.count += 1);
                                },
                            );
                        });
                    });
                }
            });
        });
    });
    let rows = ids.unwrap();
    let mut source = Source::new(app);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    paint(&mut source, &mut text, (600, 400));
    let first = records.borrow()[0];
    let second = records.borrow()[1];
    source.view.access(first).update(|v| v.count = 7);
    paint(&mut source, &mut text, (600, 400));
    assert!(contains(&source, "Local 7"));
    source.view.access(rows).update(|v| v.reverse());
    paint(&mut source, &mut text, (600, 400));
    assert_eq!(source.view.access(first).read().count, 7);
    assert_eq!(drops.get(), 0);
    // A full declaration after resize must reuse the same local slots too.
    paint(&mut source, &mut text, (800, 500));
    assert_eq!(source.view.access(first).read().count, 7);
    source.view.access(rows).set(vec![2]);
    paint(&mut source, &mut text, (800, 500));
    assert_eq!(drops.get(), 1);
    assert!(source.view.try_access(first).is_none());
    assert!(source.view.try_access(second).is_some());
    source.view.access(rows).set(vec![2, 1]);
    paint(&mut source, &mut text, (800, 500));
    assert!(source.view.try_access(first).is_none());
    assert_eq!(
        source
            .view
            .access(*records.borrow().last().unwrap())
            .read()
            .count,
        0
    );
    drop(source);
    assert_eq!(drops.get(), 3);
}

#[test]
fn allocating_local_slots_keeps_existing_short_borrows_valid() {
    let app = Regions::new(|app| {
        app.root((), |cx, ()| {
            let a = cx.local_state(|| String::from("first"));
            cx.watch((a,), |cx, (a,)| {
                let value = a.read();
                // Same owner; grow its arena while another state is borrowed.
                for key in 0..100 {
                    cx.local_state_keyed(key, || key);
                }
                text(cx, &*value);
            });
        })
    });
    let mut source = Source::new(app);
    paint(&mut source, &mut TextContext::new(), (600, 400));
    assert!(contains(&source, "first"));
}
