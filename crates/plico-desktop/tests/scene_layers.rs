use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, Scene, TextContext};

struct Layers {
    changed: bool,
}
impl View for Layers {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width(300).height(200).clip(true).opacity(0.8) + |cx| {
            row(cx).width(200).height(60).clip(true).opacity(0.7)
                .background(if self.changed { Color::Accent } else { Color::Panel }) + |cx|
            {
                text(cx, "Nested text");
            };
            text(cx, "Sibling inside outer clip");
        };
        text(cx, "Outside both clips");
    }
}

#[test]
fn scene_rebuilds_preserve_nested_layers_and_siblings() {
    let mut source = Source::with_clock(Layers { changed: false }, plico::VirtualClock::default());
    let mut text = TextContext::new();
    let mut rebuilt = Scene::new();
    source
        .render_retained_frame(&mut rebuilt, &mut text, (480, 360), 1.)
        .unwrap();
    for (index, scale) in [1., 2., 2., 1.].into_iter().enumerate() {
        source.view.changed = !source.view.changed;
        source.request_rebuild();
        source
            .render_retained_frame(&mut rebuilt, &mut text, (480, 360), scale)
            .unwrap();
        if matches!(index, 1 | 3) {
            assert_eq!(source.adapter.stats.scene_builds, 1, "DPI changed");
        }
        let nodes = source.build_frame(&mut text, 480, 360);
        let mut reference = Scene::new();
        plico_renderer::scene::build_scene_scaled_layers(
            &mut reference,
            &[&nodes],
            &mut text,
            480,
            360,
            source.base_color(),
            scale,
        );
        assert_eq!(rebuilt.commands.as_slice(), reference.commands.as_slice());
        let actual =
            plico_shell::renderer::render_to_image(&rebuilt, 480, 360, source.base_color())
                .unwrap();
        let expected =
            plico_shell::renderer::render_to_image(&reference, 480, 360, source.base_color())
                .unwrap();
        assert_eq!(actual.as_raw(), expected.as_raw());
    }
}

#[test]
fn unchanged_backdrop_is_reencoded_when_its_prefix_changes() {
    struct Backdrop(bool);
    impl View for Backdrop {
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width(80)
                .height(40)
                .background(if self.0 { Color::Accent } else { Color::Panel })
                .children(|_| {});
            column(cx)
                .width(80)
                .height(40)
                .backdrop_blur(8.)
                .children(|cx| {
                    text(cx, "Glass");
                });
        }
    }
    let mut source = Source::with_clock(Backdrop(false), plico::VirtualClock::default());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (200, 200), 1.)
        .unwrap();
    source.view.0 = true;
    source.request_rebuild();
    source
        .render_retained_frame(&mut scene, &mut text, (200, 200), 1.)
        .unwrap();
    assert_eq!(source.adapter.stats.scene_builds, 1);
    let nodes = source.build_frame(&mut text, 200, 200);
    let mut full = Scene::new();
    plico_renderer::scene::build_scene_scaled_layers(
        &mut full,
        &[&nodes],
        &mut text,
        200,
        200,
        source.base_color(),
        1.,
    );
    assert_eq!(scene.commands.as_slice(), full.commands.as_slice());
}

#[test]
fn workspace_hover_matches_full_encoding() {
    use gallery::{Gallery, Page};
    use plico_renderer::{Modifiers, Point, PointerEvent, PointerPhase, UiEvent};
    let mut source = Source::with_clock(
        Gallery {
            router: plico::Router::new(Page::Workspace),
            ..Default::default()
        },
        plico::VirtualClock::default(),
    );
    source.set_reduced_motion(true);
    // Save is disabled in the default fixture; enable it before testing hover.
    source.view.workspace.projects[0].draft.autosave = true;
    let mut text = TextContext::new();
    let nodes = source.build_frame(&mut text, 1120, 820);
    let rect = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Save changes"))
        .unwrap()
        .rect;
    let mut actual = Scene::new();
    source
        .render_retained_frame(&mut actual, &mut text, (1120, 820), 1.)
        .unwrap();
    let mut rebuilt_frames = 0;
    for index in 0..6 {
        let (x, y) = if index % 2 == 0 {
            (
                (rect[0] + rect[2]) as f64 / 2.,
                (rect[1] + rect[3]) as f64 / 2.,
            )
        } else {
            (1100., 10.)
        };
        source.handle_event(UiEvent::Pointer(PointerEvent {
            phase: PointerPhase::Move,
            position: Point { x, y },
            button: None,
            buttons: 0,
            modifiers: Modifiers::empty(),
            properties: Default::default(),
        }));
        source
            .render_retained_frame(&mut actual, &mut text, (1120, 820), 1.)
            .unwrap();
        rebuilt_frames += source.adapter.stats.scene_builds;
        let nodes = source.build_frame(&mut text, 1120, 820);
        let mut expected = Scene::new();
        plico_renderer::scene::build_scene_scaled_layers(
            &mut expected,
            &[&nodes],
            &mut text,
            1120,
            820,
            source.base_color(),
            1.,
        );
        assert_eq!(
            actual.commands.as_slice(),
            expected.commands.as_slice(),
            "hover frame {index}"
        );
    }
    assert!(rebuilt_frames > 0);
}
