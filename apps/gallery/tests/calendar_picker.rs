#![cfg(feature = "desktop")]
use gallery::{Gallery, Page};
use plico_core::{Router, VirtualClock};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_vello::Source;
use std::time::Duration;

#[test]
fn picker_closes_on_selection_and_on_reselecting_the_same_date() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: Router::new(Page::Calendar),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    source.build_frame(&mut text, 1120, 820);
    let mut label = "Pick a date";
    for _ in 0..2 {
        let trigger = source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(SemanticRole::Button, label, None)
            .unwrap()
            .id;
        assert!(source.handle_semantic_action(SemanticAction::Click { target: trigger }));
        source.build_frame(&mut text, 1120, 820);
        let tree = source.adapter.semantics(&source.engine, 0);
        let panel = tree
            .node_by_role(SemanticRole::Dialog, label, None)
            .unwrap();
        let mut pending = panel.children.clone();
        let mut target = None;
        while let Some(id) = pending.pop() {
            let node = tree.nodes.iter().find(|n| n.id == id).unwrap();
            if node.role == SemanticRole::Button && node.label.as_deref() == Some("15") {
                target = Some(id);
                break;
            }
            pending.extend_from_slice(&node.children);
        }
        let target = target.expect("day button inside popup, not the inline calendars");
        assert!(source.handle_semantic_action(SemanticAction::Click { target }));
        source.build_frame(&mut text, 1120, 820);
        clock.advance(Duration::from_secs(1)).unwrap();
        source.build_frame(&mut text, 1120, 820);
        label = "2026-10-15";
        let tree = source.adapter.semantics(&source.engine, 0);
        assert!(
            tree.node_by_role(SemanticRole::Button, label, None)
                .is_some()
        );
        assert!(tree.nodes.iter().all(|n| n.role != SemanticRole::Dialog));
    }
}
