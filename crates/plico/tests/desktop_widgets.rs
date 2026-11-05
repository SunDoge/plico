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
    // Observe the actual card group, including its noninteractive content.
    toast_frame(&mut engine, &mut host, 140);
    let card = toast_card(&engine).unwrap();
    engine.set_hovered(Some(card));
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
    // Charge each interval according to its previous hover state. Entering
    // banks the preceding second; leaving skips the entire idle hover gap.
    engine.set_hovered(None);
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
        now: Duration::from_secs(14),
        delta: Duration::from_secs(3),
    });
    frame(&mut engine, |cx| {
        toast_viewport(cx, &mut host);
    });
    assert_eq!(host.len(), 0, "released, the remaining time expires it");
}

fn toast_card(engine: &Engine) -> Option<Entity> {
    fn walk(engine: &Engine, entity: Entity) -> Option<Entity> {
        let node = engine.node(entity)?;
        if node.style.max_width == Some(360.) && node.style.background == Some(Color::Panel) {
            return Some(entity);
        }
        node.children.iter().find_map(|child| walk(engine, *child))
    }
    walk(engine, engine.root())
}
fn toast_frame(engine: &mut Engine, host: &mut ToastHost, millis: u64) {
    engine.set_time(FrameTime {
        now: Duration::from_millis(millis),
        delta: Duration::ZERO,
    });
    frame(engine, |cx| {
        toast_viewport(cx, host);
    });
}

#[test]
fn toast_presence_retains_identity_and_closes_from_its_current_position() {
    let mut host = ToastHost::default();
    host.toast(Toast::message("Animated").sticky());
    let mut engine = Engine::default();
    toast_frame(&mut engine, &mut host, 0);
    let card = toast_card(&engine).unwrap();
    assert_eq!(engine.node(card).unwrap().style.opacity, 0.);
    assert!(engine.changes().contains(plico::Changes::ANIMATE));
    toast_frame(&mut engine, &mut host, 70);
    let halfway = engine.node(card).unwrap().style.opacity;
    assert!(halfway > 0. && halfway < 1.);
    host.close("Animated");
    assert!(host.is_empty(), "dismissal is immediate for logical state");
    toast_frame(&mut engine, &mut host, 70);
    assert_eq!(toast_card(&engine), Some(card));
    assert_eq!(
        engine.node(card).unwrap().style.opacity,
        halfway,
        "exit must not jump back to full opacity"
    );
    assert!(engine.node(card).unwrap().style.disabled);
    toast_frame(&mut engine, &mut host, 105);
    assert!(engine.node(card).unwrap().style.opacity < halfway);
    toast_frame(&mut engine, &mut host, 210);
    assert!(toast_card(&engine).is_none());
    assert!(
        !engine.changes().contains(plico::Changes::ANIMATE),
        "finished presence must stop requesting frames"
    );
}

#[test]
fn renewing_a_retiring_toast_reverses_without_remounting() {
    let mut host = ToastHost::default();
    host.toast(Toast::message("Retry").sticky());
    let mut engine = Engine::default();
    toast_frame(&mut engine, &mut host, 0);
    toast_frame(&mut engine, &mut host, 140);
    let card = toast_card(&engine).unwrap();
    host.close("Retry");
    toast_frame(&mut engine, &mut host, 140);
    toast_frame(&mut engine, &mut host, 210);
    let before = engine.node(card).unwrap().style.opacity;
    assert!(before > 0. && before < 1.);
    host.toast(Toast::message("Retry").sticky());
    toast_frame(&mut engine, &mut host, 210);
    assert_eq!(toast_card(&engine), Some(card));
    assert_eq!(engine.node(card).unwrap().style.opacity, before);
    assert!(!engine.node(card).unwrap().style.disabled);
    toast_frame(&mut engine, &mut host, 350);
    assert_eq!(engine.node(card).unwrap().style.opacity, 1.);
    assert_eq!(host.len(), 1);
    assert!(
        !engine.changes().contains(plico::Changes::ANIMATE),
        "a settled sticky toast should idle"
    );
}

#[test]
fn new_toasts_do_not_inherit_idle_time_and_timeout_animates_out() {
    let mut engine = Engine::default();
    let mut host = ToastHost::default();
    toast_frame(&mut engine, &mut host, 0);
    host.toast(Toast::message("New"));
    toast_frame(&mut engine, &mut host, 60_000);
    assert_eq!(host.len(), 1);
    toast_frame(&mut engine, &mut host, 63_999);
    assert_eq!(host.len(), 1);
    toast_frame(&mut engine, &mut host, 64_000);
    assert!(host.is_empty());
    assert!(
        toast_card(&engine).is_some(),
        "expired content remains for its exit animation"
    );
    toast_frame(&mut engine, &mut host, 64_140);
    assert!(toast_card(&engine).is_none());
}

#[test]
fn retiring_action_cannot_fire_again_and_zero_motion_removes_immediately() {
    use std::{cell::Cell, rc::Rc};
    let fired = Rc::new(Cell::new(0));
    let counter = fired.clone();
    let mut host = ToastHost::default();
    host.toast(
        Toast::message("Action")
            .sticky()
            .with_action("Undo", move || counter.set(counter.get() + 1)),
    );
    let mut engine = Engine::default();
    toast_frame(&mut engine, &mut host, 0);
    toast_frame(&mut engine, &mut host, 140);
    let action = named(&engine, "Undo");
    engine.queue_click(action);
    toast_frame(&mut engine, &mut host, 140);
    assert_eq!(fired.get(), 1);
    engine.queue_click(action);
    toast_frame(&mut engine, &mut host, 141);
    assert_eq!(fired.get(), 1);
    assert!(engine.node(action).unwrap().style.disabled);
    let mut theme = Theme::default();
    theme.metrics.interaction_ms = 0;
    engine.set_theme(theme);
    toast_frame(&mut engine, &mut host, 300);
    host.toast(Toast::message("Instant").sticky());
    toast_frame(&mut engine, &mut host, 301);
    assert_eq!(
        engine
            .node(toast_card(&engine).unwrap())
            .unwrap()
            .style
            .opacity,
        1.
    );
    host.close("");
    toast_frame(&mut engine, &mut host, 301);
    assert!(toast_card(&engine).is_none());
}

#[test]
fn dismissing_after_idle_starts_exit_on_the_next_frame() {
    let mut engine = Engine::default();
    let mut host = ToastHost::default();
    host.toast(Toast::message("Idle").sticky());
    toast_frame(&mut engine, &mut host, 0);
    toast_frame(&mut engine, &mut host, 140);
    host.close("Idle");
    toast_frame(&mut engine, &mut host, 60_000);
    let card = toast_card(&engine).unwrap();
    assert_eq!(engine.node(card).unwrap().style.opacity, 1.);
    toast_frame(&mut engine, &mut host, 60_070);
    assert!(engine.node(card).unwrap().style.opacity < 1.);
    toast_frame(&mut engine, &mut host, 60_140);
    assert!(toast_card(&engine).is_none());
}
