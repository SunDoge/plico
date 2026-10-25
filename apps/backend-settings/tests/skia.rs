#![cfg(feature = "skia")]
use backend_settings::{Page, Settings};
use plico_core::VirtualClock;
use plico_renderer::{FrameSource, Scene, TextContext, scene};
use plico_runtime::Source;
use plico_skia::SkiaSurface;
use std::time::Duration;

#[test]
fn complete_settings_scenes_replay_on_skia_in_both_themes() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut text = TextContext::new();
    let mut tick = 0;
    for dark in [false, true] {
        for page in [Page::Overview, Page::Activity, Page::Preferences] {
            for (width, height) in [(1100, 820), (520, 760)] {
                source.view.dark = dark;
                source.view.advanced = true;
                source.view.inspector = true;
                source.view.router.go(page);
                source.request_rebuild();
                tick += 300;
                clock.set(Duration::from_millis(tick)).unwrap();
                let nodes = source.build_frame(&mut text, width, height);
                assert!(source.last_error.is_none());
                let mut output = Scene::new();
                scene::build_scene(
                    &mut output,
                    &nodes,
                    &mut text,
                    width,
                    height,
                    vello_common::peniko::Color::TRANSPARENT,
                );
                assert!(
                    output
                        .commands
                        .iter()
                        .any(|c| matches!(c, plico_renderer::PaintCommand::Svg { .. })),
                    "SVG icons must remain in the UI"
                );
                let mut surface = SkiaSurface::new(width, height).unwrap();
                let pixels = surface
                    .render_scene(&output, vello_common::peniko::Color::TRANSPARENT)
                    .unwrap_or_else(|e| panic!("{page:?}, dark={dark}, width={width}: {e}"));
                assert!(pixels.as_chunks::<4>().0.iter().any(|p| p != &pixels[..4]));
            }
        }
    }
}

#[test]
fn animation_frames_replay_without_changing_the_shared_state_model() {
    use plico_renderer::{SemanticAction, SemanticRole};
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut text = TextContext::new();
    let mut surface = SkiaSurface::new(1100, 820).unwrap();
    source.build_frame(&mut text, 1100, 820);
    let click = |source: &mut Source<Settings>, label| {
        let id = source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(SemanticRole::Button, label, None)
            .unwrap()
            .id;
        assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
    };
    click(&mut source, "Details");
    source.build_frame(&mut text, 1100, 820);
    let mut previous = Vec::new();
    for time in [130, 300] {
        clock.set(Duration::from_millis(time)).unwrap();
        let nodes = source.build_frame(&mut text, 1100, 820);
        let mut output = Scene::new();
        scene::build_scene(
            &mut output,
            &nodes,
            &mut text,
            1100,
            820,
            vello_common::peniko::Color::TRANSPARENT,
        );
        let pixels = surface
            .render_scene(&output, vello_common::peniko::Color::TRANSPARENT)
            .unwrap();
        if time == 130 {
            assert!(source.view.inspector_width() > 0. && source.view.inspector_width() < 248.);
        } else {
            assert_eq!(source.view.inspector_width(), 248.);
            assert_ne!(pixels, previous);
        }
        previous = pixels;
    }
    click(&mut source, "Run preview");
    source.build_frame(&mut text, 1100, 820);
    clock.set(Duration::from_millis(1500)).unwrap();
    let nodes = source.build_frame(&mut text, 1100, 820);
    assert!((source.view.progress() - 0.5).abs() < 0.001);
    let mut output = Scene::new();
    scene::build_scene(
        &mut output,
        &nodes,
        &mut text,
        1100,
        820,
        vello_common::peniko::Color::TRANSPARENT,
    );
    surface
        .draw_scene(&output, vello_common::peniko::Color::TRANSPARENT)
        .unwrap();
}
