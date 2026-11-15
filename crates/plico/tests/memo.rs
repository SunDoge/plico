use plico::{Engine, FrameTime, UiContext, component};
use std::{cell::Cell, rc::Rc, time::Duration};

fn label(engine: &Engine) -> &str {
    let child = engine.node(engine.root()).unwrap().children[0];
    engine.node(child).unwrap().text
}

#[test]
fn unchanged_dependency_skips_and_changed_dependency_rebuilds() {
    let mut engine = Engine::default();
    let builds = Cell::new(0);
    let show = |cx: &mut UiContext<'_>, revision| {
        cx.memo(revision, |cx| {
            builds.set(builds.get() + 1);
            cx.text(if revision == 1 { "one" } else { "two" });
        });
    };

    let first = engine.frame_declarations(|cx| show(cx, 1)).unwrap();
    assert_eq!(first.memo_hits, 0);
    let text = engine.node(engine.root()).unwrap().children[0];
    let second = engine.frame_declarations(|cx| show(cx, 1)).unwrap();
    assert_eq!(second.memo_hits, 1);
    assert_eq!(second.memo_nodes, 1);
    assert_eq!(second.visited, 0);
    assert_eq!(engine.node(engine.root()).unwrap().children, [text]);
    assert_eq!(builds.get(), 1);

    let changed = engine.frame_declarations(|cx| show(cx, 2)).unwrap();
    assert_eq!(changed.memo_hits, 0);
    assert_eq!(builds.get(), 2);
    assert_eq!(label(&engine), "two");
}

#[test]
fn omitting_a_memo_unmounts_it_and_drops_the_slot() {
    let mut engine = Engine::default();
    let builds = Cell::new(0);
    let show = |cx: &mut UiContext<'_>, visible| {
        if visible {
            cx.memo(0, |cx| {
                builds.set(builds.get() + 1);
                cx.text("cached");
            });
        }
    };
    engine.frame_declarations(|cx| show(cx, true)).unwrap();
    engine.frame_declarations(|cx| show(cx, false)).unwrap();
    assert!(engine.node(engine.root()).unwrap().children.is_empty());
    engine.frame_declarations(|cx| show(cx, true)).unwrap();
    assert_eq!(builds.get(), 2);
}

#[component]
fn memo_list(cx: &mut UiContext<'_>, ids: &[u64], builds: &Cell<u32>) {
    for id in ids {
        cx.keyed(*id, |cx| {
            cx.memo(0, |cx| {
                builds.set(builds.get() + 1);
                cx.text(if *id == 1 { "one" } else { "two" });
            });
        });
    }
}

#[test]
fn keyed_memos_reuse_rows_and_reorder_their_roots() {
    let mut engine = Engine::default();
    let builds = Cell::new(0);
    engine
        .frame_declarations(|cx| memo_list(cx, &[1, 2], &builds))
        .unwrap();
    let original = engine.node(engine.root()).unwrap().children.to_vec();
    let stats = engine
        .frame_declarations(|cx| memo_list(cx, &[2, 1], &builds))
        .unwrap();
    assert_eq!(stats.memo_hits, 2);
    assert_eq!(builds.get(), 2);
    assert_eq!(
        engine.node(engine.root()).unwrap().children,
        [original[1], original[0]]
    );
}

#[test]
fn pending_activation_rebuilds_an_interactive_memo() {
    let mut engine = Engine::default();
    let builds = Cell::new(0);
    let clicks = Cell::new(0);
    let show = |cx: &mut UiContext<'_>| {
        cx.memo(0, |cx| {
            builds.set(builds.get() + 1);
            if cx.button("go").clicked() {
                clicks.set(clicks.get() + 1);
            }
        });
    };
    engine.frame_declarations(show).unwrap();
    let button = engine.node(engine.root()).unwrap().children[0];
    engine.queue_click(button);
    assert_eq!(engine.frame_declarations(show).unwrap().memo_hits, 0);
    assert_eq!(clicks.get(), 1);
    assert_eq!(builds.get(), 2);
}

#[test]
fn memo_keeps_component_local_state_until_the_dependency_changes() {
    let mut engine = Engine::default();
    let initializations = Rc::new(Cell::new(0));
    let show = |cx: &mut UiContext<'_>, revision| {
        let initializations = Rc::clone(&initializations);
        cx.memo(revision, |cx| {
            let state = cx.state(|| {
                initializations.set(initializations.get() + 1);
                String::from("state")
            });
            assert_eq!(state.read().as_str(), "state");
            cx.text("content");
        });
    };
    engine.frame_declarations(|cx| show(cx, 1)).unwrap();
    assert_eq!(
        engine
            .frame_declarations(|cx| show(cx, 1))
            .unwrap()
            .memo_hits,
        1
    );
    engine.frame_declarations(|cx| show(cx, 2)).unwrap();
    assert_eq!(initializations.get(), 1);
}

#[test]
fn animated_memos_rebuild_and_deadlines_wake_static_memos() {
    let mut engine = Engine::default();
    let animated_builds = Cell::new(0);
    let animated = |cx: &mut UiContext<'_>| {
        cx.memo(0, |cx| {
            animated_builds.set(animated_builds.get() + 1);
            cx.request_animation_frame();
            cx.text("animated");
        });
    };
    engine.frame_declarations(animated).unwrap();
    assert_eq!(engine.frame_declarations(animated).unwrap().memo_hits, 0);
    assert_eq!(animated_builds.get(), 2);

    let mut engine = Engine::default();
    let builds = Cell::new(0);
    let deadline = Duration::from_millis(20);
    let show = |cx: &mut UiContext<'_>| {
        cx.memo(0, |cx| {
            builds.set(builds.get() + 1);
            cx.request_rebuild_at(deadline);
            cx.text("deadline");
        });
    };
    engine.set_time(FrameTime {
        now: Duration::from_millis(10),
        ..Default::default()
    });
    engine.frame_declarations(show).unwrap();
    assert_eq!(engine.frame_declarations(show).unwrap().memo_hits, 1);
    engine.set_time(FrameTime {
        now: deadline,
        ..Default::default()
    });
    assert_eq!(engine.frame_declarations(show).unwrap().memo_hits, 0);
    assert_eq!(builds.get(), 2);
}

#[test]
fn a_nested_memo_deadline_invalidates_its_skipped_outer_scope() {
    let mut engine = Engine::default();
    let builds = Cell::new(0);
    let deadline = Duration::from_millis(20);
    let show = |cx: &mut UiContext<'_>| {
        cx.memo(0, |cx| {
            cx.memo(0, |cx| {
                builds.set(builds.get() + 1);
                cx.request_rebuild_at(deadline);
                cx.text("nested deadline");
            });
        });
    };
    engine.set_time(FrameTime {
        now: Duration::from_millis(10),
        ..Default::default()
    });
    engine.frame_declarations(show).unwrap();
    assert_eq!(engine.frame_declarations(show).unwrap().memo_hits, 1);
    engine.set_time(FrameTime {
        now: deadline,
        ..Default::default()
    });
    assert_eq!(engine.frame_declarations(show).unwrap().memo_hits, 0);
    assert_eq!(builds.get(), 2);
}
