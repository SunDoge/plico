#![cfg(all(feature = "wgpu", target_os = "linux"))]
use plico::prelude::*;
use plico_desktop::{HostView, PreviewScenario, RenderBackend, capture};
use plico_renderer::{FrameSource, TextContext, layout::PlacedNode};
use std::{cell::Cell, rc::Rc};

#[derive(Default, serde::Deserialize)]
struct Fixture {
    count: u32,
}
struct Demo {
    count: u32,
    observed: Rc<Cell<(u32, u128)>>,
}
impl HostView for Demo {
    fn set_frame_time(&mut self, time: plico::FrameTime) {
        self.observed.set((self.count, time.now.as_millis()));
    }
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(12).gap(8).children(|cx| {
            text!(cx, "Count {} · 日本語", self.count);
            if button(cx).text("Increment").clicked() {
                self.count += 1;
            }
        });
    }
}
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn wgpu_preview_replays_seeded_actions_and_virtual_clock_repeatably() {
    let json = r#"{"state":{"count":4},"steps":[{"after_ms":10,"command":{"action":"click","role":"button","label":"Increment"}}],"time_ms":200,"viewport":{"width":320,"height":240,"scale":1.5}}"#;
    let render = || {
        let observed = Rc::new(Cell::new((0, 0)));
        let scenario: PreviewScenario<Fixture> = serde_json::from_str(json).unwrap();
        let image = scenario
            .render_with_backend(RenderBackend::Wgpu, |state| Demo {
                count: state.count,
                observed: observed.clone(),
            })
            .unwrap();
        assert_eq!(observed.get(), (5, 210));
        assert_eq!((image.width(), image.height()), (480, 360));
        assert!(
            image
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[..3] != [0, 0, 0])
        );
        image
    };
    assert_eq!(render(), render());
}
struct Empty;
impl FrameSource for Empty {
    fn build_frame(&mut self, _: &mut TextContext, _: u32, _: u32) -> Vec<PlacedNode> {
        Vec::new()
    }
    // This fixture tests the renderer clear itself; the default FrameSource
    // painter also emits a background rectangle, so intentionally emit no scene.
    fn paint_scene(
        &mut self,
        _: &mut plico_renderer::Scene,
        _: &[PlacedNode],
        _: &mut TextContext,
        _: (u32, u32),
        _: f64,
    ) {
    }
    fn base_color(&self) -> peniko::Color {
        peniko::Color::from_rgba8(255, 0, 0, 128)
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn wgpu_capture_preserves_premultiplied_pixels_and_writes_straight_alpha_png() {
    let image = capture(&mut Empty)
        .renderer(RenderBackend::Wgpu)
        .size(80, 40)
        .scale(1.25)
        .render()
        .unwrap();
    assert_eq!((image.width(), image.height()), (100, 50));
    assert!(
        image
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [128, 0, 0, 128])
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("preview.png");
    image.write_png(&path).unwrap();
    let decoded = image::open(path).unwrap().to_rgba8();
    assert!(decoded.pixels().all(|pixel| pixel.0 == [255, 0, 0, 128]));
}
#[test]
fn capture_rejects_unavailable_backend_without_silent_fallback() {
    assert!(matches!(
        capture(&mut Empty).renderer(RenderBackend::Vello).render(),
        Err(plico_desktop::CaptureError::Backend {
            backend: RenderBackend::Vello
        })
    ));
    assert!(capture(&mut Empty).size(0, 40).render().is_err());
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn wgpu_preview_captures_a_hosted_image_canvas_with_the_same_backend() {
    use plico_renderer::{
        image::RasterImage,
        image_canvas::{ImageCanvas, ImageCanvasState},
    };
    use std::sync::Arc;
    struct Canvas;
    impl HostView for Canvas {
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).key("canvas").width(100.).height(100.).end();
        }
    }
    let scenario: PreviewScenario<()> =
        serde_json::from_str(r#"{"viewport":{"width":100,"height":100,"scale":1.25}}"#).unwrap();
    let image = scenario
        .render_source_with_backend(RenderBackend::Wgpu, |(), clock| {
            let state = ImageCanvasState::default();
            state.set_document(
                Some(Arc::new(RasterImage::from_rgba(
                    image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 255, 0, 255])),
                ))),
                vec![],
            );
            plico_desktop::SurfaceSource::with_clock(
                Canvas,
                ImageCanvas::new(state),
                "canvas",
                clock,
            )
        })
        .unwrap();
    assert_eq!((image.width(), image.height()), (125, 125));
    assert!(image.rgba().as_chunks::<4>().0.contains(&[0, 255, 0, 255]));
}
