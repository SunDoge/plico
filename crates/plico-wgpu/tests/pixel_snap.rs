use kurbo::Affine;
use peniko::Color as PaintColor;
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;

#[test]
#[ignore = "requires a working wgpu adapter"]
fn fractional_dpi_checkbox_rim_snaps_to_one_physical_pixel() {
    let mut scene = Scene::new();
    scene.stroke(
        &kurbo::Stroke::new(1.),
        Affine::translate((4.375, 4.375)) * Affine::scale(1.25),
        PaintColor::from_rgba8(255, 255, 255, 255),
        None,
        &kurbo::RoundedRect::new(0.5, 0.5, 15.5, 15.5, 3.5),
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&scene, [32, 32], [0.; 4], false)
        .unwrap();
    let alpha = |x: usize, y: usize| pixels[(y * 32 + x) * 4 + 3];
    // The physical outer rectangle snaps to [4, 24) on both axes. Sample
    // perpendicular to each straight edge, clear on either side of the rim.
    for along in 10..18 {
        for (cross, expected) in [(3, 0), (4, 255), (5, 0), (22, 0), (23, 255), (24, 0)] {
            assert_eq!(
                alpha(along, cross),
                expected,
                "horizontal rim at {along},{cross}"
            );
            assert_eq!(
                alpha(cross, along),
                expected,
                "vertical rim at {cross},{along}"
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn checkbox_click_preserves_four_edges_and_draws_both_check_strokes() {
    use plico::prelude::*;
    use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
    use plico_runtime::{HostView, Source};
    use std::time::Duration;

    struct Choice(bool);
    impl HostView for Choice {
        fn show(&mut self, cx: &mut UiContext) {
            checkbox(cx, "Choice", &mut self.0);
        }

        fn theme(&self) -> Theme {
            Theme {
                accent: 0x0000ff,
                on_accent: 0x00ff00,
                foreground: 0,
                border: 0,
                ..Theme::light()
            }
        }
    }

    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(Choice(false), clock.clone());
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    let mut render = |source: &mut Source<Choice>| {
        source
            .try_build_and_paint(&mut scene, &mut text, 96, 48, 1.25)
            .unwrap();
        renderer
            .render_scene(&scene, [120, 60], [0.; 4], false)
            .unwrap()
    };
    let off = render(&mut source);
    let target = source
        .semantics()
        .node_by_role(SemanticRole::CheckBox, "Choice", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target }));
    render(&mut source);
    clock.advance(Duration::from_millis(40)).unwrap();
    let fading = render(&mut source);
    clock.advance(Duration::from_millis(210)).unwrap();
    let on = render(&mut source);
    assert!(source.view().0);

    // Only the checked face is blue. Locate it independently of platform text metrics.
    let mut left = usize::MAX;
    let mut top = usize::MAX;
    for (i, pixel) in on.as_chunks::<4>().0.iter().copied().enumerate() {
        if pixel == [0, 0, 255, 255] {
            left = left.min(i % 120);
            top = top.min(i / 120);
        }
    }
    assert!(left < 101 && top < 41, "missing checked face: {left},{top}");
    let pixel = |frame: &[u8], x, y| -> [u8; 4] {
        let index: usize = ((top + y) * 120 + left + x) * 4;
        frame[index..index + 4].try_into().unwrap()
    };
    // Default face is 16 DIP, or 20 physical pixels at this scale.
    for along in 6..14 {
        for (x, y) in [(0, along), (19, along), (along, 0), (along, 19)] {
            assert_eq!(pixel(&off, x, y), [0, 0, 0, 255], "unchecked rim {x},{y}");
            assert_eq!(pixel(&on, x, y), [0, 0, 255, 255], "checked rim {x},{y}");
        }
    }
    let contrast = |frame: &[u8]| pixel(frame, 7, 6)[2].saturating_sub(pixel(frame, 8, 13)[2]);
    assert_eq!(contrast(&off), 0);
    assert!(
        contrast(&fading) > 0 && contrast(&fading) < contrast(&on),
        "check must fade in"
    );
    for (x, y) in [(5, 11), (8, 13), (14, 7)] {
        assert!(pixel(&on, x, y)[1] > 128, "missing check stroke at {x},{y}");
    }
    assert_eq!(
        pixel(&on, 7, 6),
        [0, 0, 255, 255],
        "mark filled the upper interior"
    );
    assert!(source.handle_semantic_action(SemanticAction::Click { target }));
    render(&mut source);
    clock.advance(Duration::from_millis(250)).unwrap();
    assert!(render(&mut source) == off, "unchecking left stale pixels");
}
