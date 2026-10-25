use gallery::{Gallery, Page};
use plico_core::{Clock, VirtualClock};
use plico_renderer::{FrameSource, PaintCommand, Scene, TextContext};
use plico_vello::Source;
use std::time::Duration;
#[test]
fn glass_background_moves_without_layout_and_pause_returns_to_idle() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Glass),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1120, 820);
    clock.advance(Duration::from_secs(1)).unwrap();
    let nodes = source.build_frame(&mut tcx, 1120, 820);
    assert!(source.last_error.is_none());
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert_eq!(source.adapter.stats.text_measure_invalidations, 0);
    assert!(source.has_anim());
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, &mut tcx, (1120, 820), 1.);
    assert!(scene.commands.iter().any(|c| matches!(
        c,
        PaintCommand::PushLayer {
            filter: Some(_),
            ..
        }
    )));
    source.view.glass.moving = false;
    clock.advance(Duration::from_secs(1)).unwrap();
    source.build_frame(&mut tcx, 1120, 820);
    // The checkbox itself has a short checked-state transition.
    clock.advance(Duration::from_millis(240)).unwrap();
    source.build_frame(&mut tcx, 1120, 820);
    assert!(!source.has_anim());
    source.view.glass.blur = 8.;
    source.build_frame(&mut tcx, 1120, 820);
    // The numeric slider label also changes width; test paint-only invalidation
    // independently in the backend fixture without changing any text.
    assert!(
        source
            .adapter
            .placed
            .iter()
            .any(|p| p.paint.backdrop_blur == 8.)
    );
    assert!(clock.now() >= Duration::from_secs(2));
}
