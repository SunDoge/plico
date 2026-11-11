use gallery::{Gallery, Page};
use plico::{Clock, VirtualClock};
use plico_desktop::Source;
use plico_renderer::{FrameSource, TextContext};
use std::time::Duration;
#[test]
fn reply_finishes_keeps_rich_paragraphs_and_becomes_idle() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: plico::Router::new(Page::Markdown),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1120, 820);
    for _ in 0..160 {
        clock.advance(Duration::from_millis(100)).unwrap();
        source.build_frame(&mut tcx, 1120, 820);
        assert!(source.last_error().is_none(), "{:?}", source.last_error());
    }
    for p in &source.adapter.placed {
        if p.paint
            .text
            .as_deref()
            .is_some_and(|t| t.starts_with("A good"))
        {
            assert!(p.rect[2] <= 1096.);
            let layout = plico_renderer::scene::layout_node_text(&mut tcx, p).unwrap();
            assert!(
                layout.width() <= p.content_size[0] + 1.,
                "{} > {}",
                layout.width(),
                p.content_size[0]
            );
        }
    }
    assert!(!source.view.markdown_chat.stream.is_receiving());
    assert!(!source.view.markdown_chat.stream.is_revealing());
    assert_eq!(
        source.view.markdown_chat.stream.received(),
        gallery::markdown_chat::REPLY
    );
    assert!(
        source
            .adapter
            .placed
            .iter()
            .any(|p| !p.paint.text_runs.is_empty())
    );
    assert!(!source.engine.changes().contains(plico::Changes::ANIMATE));
    let before = source.view.markdown_chat.stream.document().parsed_bytes;
    clock.advance(Duration::from_secs(1)).unwrap();
    source.build_frame(&mut tcx, 1120, 820);
    assert_eq!(
        before,
        source.view.markdown_chat.stream.document().parsed_bytes
    );
    assert_eq!(source.adapter.stats.created, 0);
    assert!(!source.has_anim());
    assert!(clock.now() >= Duration::from_secs(16));
}

#[test]
fn scrolling_up_releases_follow_instead_of_pulling_reader_back() {
    use plico_renderer::{GesturePhase, Modifiers, Point, UiEvent, WheelDeltaMode, WheelEvent};
    for (delta, milliseconds) in [(-150., 80), (-0.5, 1)] {
        let clock = VirtualClock::default();
        let mut gallery = Gallery {
            router: plico::Router::new(Page::Markdown),
            ..Default::default()
        };
        gallery.markdown_chat.burst();
        gallery.markdown_chat.stream.reduced_motion = true;
        let mut source = Source::with_clock(gallery, clock.clone());
        let mut tcx = TextContext::new();
        for _ in 0..4 {
            source.build_frame(&mut tcx, 900, 700);
        }
        // The sidebar is independently scrollable too. Target the transcript's
        // content parent, rather than whichever scroll port was declared first.
        let content_parent = source
            .adapter
            .placed
            .iter()
            .find(|p| p.paint.text.as_deref() == Some("A reply that arrives at its own pace."))
            .unwrap()
            .parent_node_id;
        let viewport = source
            .adapter
            .placed
            .iter()
            .find(|p| p.scroll.range[1] > 0. && p.parent_node_id == content_parent)
            .unwrap();
        let position = Point {
            x: f64::from(viewport.scroll.port[0] + 20.),
            y: f64::from(viewport.scroll.port[1] + 20.),
        };
        assert!(viewport.scroll.offset[1] > 0.);
        let port_id = viewport.node_id;
        let bottom = viewport.scroll.offset[1];
        source.handle_event(UiEvent::Wheel(WheelEvent {
            position,
            delta_x: 0.,
            delta_y: delta,
            delta_mode: WheelDeltaMode::Pixel,
            phase: GesturePhase::Changed,
            modifiers: Modifiers::empty(),
        }));
        clock.advance(Duration::from_millis(milliseconds)).unwrap();
        source.build_frame(&mut tcx, 900, 700);
        assert!(!source.view.markdown_chat.follow);
        for _ in 0..12 {
            clock.advance(Duration::from_millis(16)).unwrap();
            source.build_frame(&mut tcx, 900, 700);
        }
        let offset = source
            .adapter
            .placed
            .iter()
            .find(|p| p.node_id == port_id)
            .unwrap()
            .scroll
            .offset[1];
        assert!(
            offset < bottom,
            "scroll remains above the bottom after settling"
        );
        let target = source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(plico_renderer::SemanticRole::CheckBox, "Follow reply", None)
            .unwrap()
            .id;
        assert!(source.handle_semantic_action(plico_renderer::SemanticAction::Click { target }));
        source.build_frame(&mut tcx, 900, 700);
        assert!(source.view.markdown_chat.follow);
        let viewport = source
            .adapter
            .placed
            .iter()
            .find(|p| p.node_id == port_id)
            .unwrap();
        assert!(
            (viewport.scroll.offset[1] - viewport.scroll.range[1]).abs() < 0.1,
            "re-enabling follow returns to the end"
        );
    }
}
