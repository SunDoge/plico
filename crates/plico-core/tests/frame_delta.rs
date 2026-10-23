//! Contracts of `Engine::take_frame_delta` for incremental backends.
use plico_core::{Changes, DeclareView, Engine, FrameDelta, prelude::*};

fn settle(engine: &mut Engine, view: &mut impl DeclareView) -> FrameDelta {
    engine.declare_until_settled(view).unwrap();
    engine.take_frame_delta()
}
#[test]
fn steady_state_yields_an_empty_delta() {
    struct Idle;
    impl DeclareView for Idle {
        fn theme(&self) -> plico_core::Theme {
            plico_core::Theme::default()
        }
        fn show(&mut self, cx: &mut UiContext) {
            let _ = column(cx).gap(4.)
                + |cx| {
                    text(cx, "Stable");
                };
        }
    }
    let mut engine = Engine::default();
    let mut view = Idle;
    settle(&mut engine, &mut view);
    let delta = settle(&mut engine, &mut view);
    assert!(delta.changes.is_empty());
    assert!(delta.removed.is_empty());
    assert_eq!(delta.frame, Changes::empty());
}
#[test]
fn created_nodes_appear_with_all_channels_and_once_only() {
    struct Growing {
        extra: bool,
    }
    impl DeclareView for Growing {
        fn theme(&self) -> plico_core::Theme {
            plico_core::Theme::default()
        }
        fn show(&mut self, cx: &mut UiContext) {
            let _ = column(cx)
                + |cx| {
                    text(cx, "Base");
                    if self.extra {
                        text(cx, "Added").color(plico_core::Color::Literal(0xff0000));
                    }
                };
        }
    }
    let mut engine = Engine::default();
    let mut view = Growing { extra: false };
    settle(&mut engine, &mut view);
    // Growing the tree reports the new leaf with structural channels and
    // the parent whose child list changed — a backend needs both.
    view.extra = true;
    let delta = settle(&mut engine, &mut view);
    assert_eq!(delta.changes.len(), 2, "{:?}", delta.changes);
    let all = delta
        .changes
        .iter()
        .fold(Changes::empty(), |acc, c| acc | c.changes);
    assert!(
        all.contains(Changes::STRUCTURE | Changes::LAYOUT | Changes::PAINT | Changes::SEMANTICS)
    );
    // Taking consumed it: the next settle of an unchanged view sees nothing.
    let delta = settle(&mut engine, &mut view);
    assert!(delta.changes.is_empty(), "{:?}", delta.changes);
}
#[test]
fn style_and_text_updates_union_across_passes_and_removals_are_reported() {
    struct Document {
        color: plico_core::Color,
        hide: bool,
        label: &'static str,
    }
    impl DeclareView for Document {
        fn theme(&self) -> plico_core::Theme {
            plico_core::Theme::default()
        }
        fn show(&mut self, cx: &mut UiContext) {
            let _ = column(cx)
                + |cx| {
                    if !self.hide {
                        text(cx, self.label).color(self.color);
                    }
                    text(cx, "Keep");
                };
        }
    }
    let mut engine = Engine::default();
    let mut view = Document {
        color: plico_core::Color::Literal(0x222222),
        hide: false,
        label: "Old",
    };
    settle(&mut engine, &mut view);
    // A text change invalidates measurement and layout; a later pure color
    // change is paint-only. Both deltas report exactly one node.
    view.label = "New";
    let first = settle(&mut engine, &mut view);
    view.color = plico_core::Color::Literal(0xff0000);
    let second = settle(&mut engine, &mut view);
    assert_eq!(first.changes.len(), 1, "{:?}", first.changes);
    assert_eq!(second.changes.len(), 1, "{:?}", second.changes);
    assert!(
        first.changes[0]
            .changes
            .contains(Changes::MEASURE | Changes::LAYOUT | Changes::PAINT)
    );
    assert_eq!(second.changes[0].changes, Changes::PAINT);
    // Unmounting a subtree reports the entity in `removed` and nothing stale
    // in `changes`.
    view.hide = true;
    let delta = settle(&mut engine, &mut view);
    assert_eq!(delta.removed.len(), 1, "{:?}", delta.removed);
    assert!(
        delta
            .changes
            .iter()
            .all(|c| delta.removed != vec![c.entity]),
        "removed entity must not also appear as changed: {:?}",
        delta.changes
    );
}
