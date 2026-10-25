use gallery::{Gallery, Page};
use plico_core::{Clock, VirtualClock};
use plico_renderer::{FrameSource, TextContext};
use plico_vello::Source;
use std::time::Duration;
#[test]
fn reply_finishes_keeps_rich_paragraphs_and_becomes_idle() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Markdown),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1120, 820);
    for _ in 0..160 {
        clock.advance(Duration::from_millis(100)).unwrap();
        source.build_frame(&mut tcx, 1120, 820);
        assert!(source.last_error.is_none(), "{:?}", source.last_error);
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
    assert!(
        !source
            .engine
            .changes()
            .contains(plico_core::Changes::ANIMATE)
    );
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
    let clock = VirtualClock::default();
    let mut gallery = Gallery {
        router: plico_core::Router::new(Page::Markdown),
        ..Default::default()
    };
    gallery.markdown_chat.burst();
    gallery.markdown_chat.stream.reduced_motion = true;
    let mut source = Source::with_clock(gallery, clock.clone());
    let mut tcx = TextContext::new();
    for _ in 0..4 {
        source.build_frame(&mut tcx, 900, 700);
    }
    let viewport = source
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.range[1] > 0.)
        .unwrap();
    let position = Point {
        x: f64::from(viewport.scroll.port[0] + 20.),
        y: f64::from(viewport.scroll.port[1] + 20.),
    };
    assert!(viewport.scroll.offset[1] > 0.);
    source.handle_event(UiEvent::Wheel(WheelEvent {
        position,
        delta_x: 0.,
        delta_y: -150.,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Modifiers::empty(),
    }));
    clock.advance(Duration::from_millis(80)).unwrap();
    source.build_frame(&mut tcx, 900, 700);
    assert!(!source.view.markdown_chat.follow);
}
