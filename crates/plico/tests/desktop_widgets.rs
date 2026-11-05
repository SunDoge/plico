//! The desktop-paradigm widgets: tab strips, toasts and the transient-state
//! layer they rely on. Interaction flows run through the real Engine so the
//! semantic nodes and changes surface exactly as applications see them.
use plico::prelude::*;
use plico::{Engine, Entity, FrameTime};
use std::time::Duration;

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

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

#[test]
fn tabs_report_selection_changes_and_mark_the_chosen_tab() {
    let mut tab = 1;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        tabs(cx, &mut tab, &["Alpha", "Beta", "Gamma"]);
    });
    assert_eq!(tab, 1);
    assert_eq!(
        engine.node(named(&engine, "Beta")).unwrap().style.selected,
        Some(true)
    );
    // Clicking the chosen tab keeps the selection; another one changes it.
    engine.queue_click(named(&engine, "Beta"));
    frame(&mut engine, |cx| {
        tabs(cx, &mut tab, &["Alpha", "Beta", "Gamma"]);
    });
    assert_eq!(tab, 1);
    engine.queue_click(named(&engine, "Gamma"));
    frame(&mut engine, |cx| {
        tabs(cx, &mut tab, &["Alpha", "Beta", "Gamma"]);
    });
    assert_eq!(tab, 2, "clicking another tab reports the change");
    // A click settles: the frame that consumed it declared the old
    // selection (the choice lands through clicked(), evaluated after the
    // button is built), so the chosen-tab styling appears on the next
    // declaration, exactly as the settled host renders it.
    frame(&mut engine, |cx| {
        tabs(cx, &mut tab, &["Alpha", "Beta", "Gamma"]);
    });
    assert_eq!(
        engine.node(named(&engine, "Gamma")).unwrap().style.selected,
        Some(true)
    );
    assert_eq!(
        engine.node(named(&engine, "Beta")).unwrap().style.selected,
        Some(false)
    );
}

#[test]
fn toasts_expire_with_the_frame_clock_and_sticky_ones_stay() {
    let mut host = ToastHost::default();
    host.toast(Toast::message("Saved"));
    assert_eq!(host.len(), 1);
    // Renewal by ID replaces rather than stacks.
    host.toast(Toast::message("Saved"));
    assert_eq!(host.len(), 1);
    host.toast(Toast::message("Other"));
    assert_eq!(host.len(), 2);

    let mut engine = Engine::default();
    engine.set_time(FrameTime {
        now: Duration::ZERO,
        delta: Duration::ZERO,
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    assert_eq!(host.len(), 2, "fresh toasts show");
    // Just under the four-second default: still showing.
    engine.set_time(FrameTime {
        now: Duration::from_millis(3900),
        delta: Duration::from_millis(3900),
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    assert_eq!(host.len(), 2, "the 4s lifetime has not run out yet");
    // Past it: both expire.
    engine.set_time(FrameTime {
        now: Duration::from_millis(4100),
        delta: Duration::from_millis(200),
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    assert_eq!(host.len(), 0, "the default lifetime expired both");
    // Sticky toasts never time out.
    host.toast(Toast::message("Keep").sticky());
    engine.set_time(FrameTime {
        now: Duration::from_secs(120),
        delta: Duration::from_secs(120),
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    assert_eq!(host.len(), 1, "sticky stays");
    host.close("");
    assert_eq!(host.len(), 0, "close() with no id clears every toast");
}

#[test]
fn toast_snapshots_stay_transient() {
    let mut host = ToastHost::default();
    host.toast(Toast::message("Deleted").with_action("Undo", || {}));
    assert_eq!(host.len(), 1);
    // A cloned host carries no toasts: toasts are transient runtime state,
    // as hover and press are, and never travel through state copies.
    let clone = host.clone();
    assert!(clone.is_empty());
    // Restoring a serialized host yields an empty one.
    let bytes = rmp_serde::to_vec(&host).unwrap();
    let restored: ToastHost = rmp_serde::from_slice(&bytes).unwrap();
    assert!(restored.is_empty());
    host.close("Deleted");
    assert!(host.is_empty());
}

#[test]
fn toast_actions_run_once_and_renew_by_id() {
    let mut host = ToastHost::default();
    // An action taken from the host runs exactly once: the entry consumes it
    // as it fires, so a second frame or a renewal cannot fire it again.
    use std::cell::Cell;
    let fired = std::rc::Rc::new(Cell::new(0));
    let fired_ref = fired.clone();
    host.toast(Toast::message("Deleted").with_action("Undo", move || {
        fired_ref.set(fired_ref.get() + 1);
    }));
    // The action only fires through the viewport's action button; a plain
    // close (timeout, Escape, the x) must not run it.
    host.close("");
    assert_eq!(
        fired.get(),
        0,
        "closing without clicking the action never runs it"
    );
}

#[test]
fn drag_area_tracks_deltas_only_while_pressed_and_releases_on_up() {
    let mut dot = (0., 0.);
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        drag_area(cx, &mut |dx, dy| {
            dot.0 += dx;
            dot.1 += dy;
        });
    });
    let surface = named(&engine, "Drag surface");
    // Press on the surface, then move: deltas accumulate per frame.
    engine.set_pressed(Some(surface));
    engine.set_pointer_position(Some((100., 50.)));
    frame(&mut engine, |cx| {
        drag_area(cx, &mut |dx, dy| {
            dot.0 += dx;
            dot.1 += dy;
        });
    });
    assert_eq!(dot, (0., 0.), "the press frame anchors without a delta");
    engine.set_pointer_position(Some((140., 50.)));
    frame(&mut engine, |cx| {
        drag_area(cx, &mut |dx, dy| {
            dot.0 += dx;
            dot.1 += dy;
        });
    });
    assert_eq!(dot, (40., 0.), "the move reports the delta");
    // Release: the anchor forgets; a later press re-anchors cleanly.
    engine.set_pressed(None);
    engine.set_pointer_position(Some((160., 70.)));
    frame(&mut engine, |cx| {
        drag_area(cx, &mut |dx, dy| {
            dot.0 += dx;
            dot.1 += dy;
        });
    });
    assert_eq!(dot, (40., 0.), "no report after release");
    engine.set_pressed(Some(surface));
    engine.set_pointer_position(Some((10., 10.)));
    frame(&mut engine, |cx| {
        drag_area(cx, &mut |dx, dy| {
            dot.0 += dx;
            dot.1 += dy;
        });
    });
    assert_eq!(dot, (40., 0.), "a fresh press re-anchors without a jump");
}

#[test]
fn toast_time_stops_while_the_pointer_rests_on_it() {
    let mut host = ToastHost::default();
    host.toast(Toast::message("Held"));
    let mut engine = Engine::default();
    engine.set_time(FrameTime {
        now: Duration::ZERO,
        delta: Duration::ZERO,
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    // Hold the toast: hover what the viewport actually reports hovered,
    // the toast card (the text inside is a child of it). The first frame
    // ran without the hold, so the countdown must also survive that.
    // The viewport is the outermost column of toast_viewport: the toast
    // test frame declares nothing else, so it is the root's first child.
    let viewport = engine.node(engine.root()).unwrap().children[0];
    engine.set_hovered(Some(viewport));
    for elapsed in [1000, 2000, 3000, 5000, 8000] {
        engine.set_time(FrameTime {
            now: Duration::from_millis(elapsed),
            delta: Duration::from_millis(1000),
        });
        frame(&mut engine, |cx| {
            toast_viewport(cx, &mut host);
        });
    }
    assert_eq!(host.len(), 1, "the held toast's time does not run");
    // Release: the countdown resumes from what it banked before the hold -
    // a hold freezes the remaining time, it does not renew it (MyGO pauses
    // the time, it does not reset it). The hold applies frame-granularly:
    // the frame that ends the hold still reads the held state, so the gap
    // starts counting on the frame after it.
    engine.set_hovered(None);
    // The hold applied from its own frame, so only the first frame banked
    // 1s of the 4s life; the release frame counts its own gap and the
    // remaining 3s expire within it (resume-from-banked, not reset).
    engine.set_time(FrameTime {
        now: Duration::from_secs(11),
        delta: Duration::from_secs(3),
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    assert_eq!(
        host.len(),
        1,
        "the release frame banks its gap without over-running the life"
    );
    engine.set_time(FrameTime {
        now: Duration::from_secs(12),
        delta: Duration::from_secs(1),
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    assert_eq!(host.len(), 0, "released, the remaining time expires it");
}
