#![cfg(feature = "skia")]
use plico::prelude::*;
use plico_desktop::{HostView, PreviewScenario};
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
        text!(cx, "Count {}", self.count);
        if button(cx).text("Increment").clicked() {
            self.count += 1;
        }
    }
}
#[test]
fn seeded_actions_and_virtual_time_produce_repeatable_pixels() {
    let json = r#"{"state":{"count":4},"steps":[{"after_ms":10,"command":{"action":"click","role":"button","label":"Increment"}}],"time_ms":200,"viewport":{"width":320,"height":240,"scale":1.5}}"#;
    let render = || {
        let observed = Rc::new(Cell::new((0, 0)));
        let scenario: PreviewScenario<Fixture> = serde_json::from_str(json).unwrap();
        let frame = scenario
            .render(|s| Demo {
                count: s.count,
                observed: observed.clone(),
            })
            .unwrap();
        assert_eq!(observed.get(), (5, 210));
        assert_eq!((frame.width(), frame.height()), (480, 360));
        frame
    };
    assert_eq!(render(), render());
}
#[test]
fn invalid_commands_versions_and_dimensions_are_errors() {
    for json in [
        r#"{"version":2}"#,
        r#"{"viewport":{"width":0}}"#,
        r#"{"steps":[{"command":{"action":"click","role":"button","label":"missing"}}]}"#,
    ] {
        let scenario: PreviewScenario<Fixture> = serde_json::from_str(json).unwrap();
        assert!(
            scenario
                .render(|s| Demo {
                    count: s.count,
                    observed: Default::default()
                })
                .is_err()
        );
    }
    assert!(serde_json::from_str::<PreviewScenario<Fixture>>(r#"{"typo":true}"#).is_err());
}
#[test]
fn ordinary_apps_do_not_require_serialization() {
    #[derive(Default)]
    struct Ordinary {
        _not_serializable: std::sync::Mutex<u32>,
    }
    impl plico_dev::App for Ordinary {
        fn show(&mut self, cx: &mut UiContext) {
            text(cx, "ordinary state");
        }
    }
    plico_desktop::PreviewScenario::<()>::default()
        .render(|()| plico_desktop::NativeApp(Ordinary::default()))
        .unwrap();
}

#[test]
fn native_image_surface_is_present_in_scenario_capture() {
    use plico_renderer::{
        image::RasterImage,
        image_canvas::{ImageCanvas, ImageCanvasState},
    };
    use std::sync::Arc;
    struct CanvasView;
    impl HostView for CanvasView {
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).key("canvas").width(100.).height(100.).end();
        }
    }
    let scenario: PreviewScenario<()> =
        serde_json::from_str(r#"{"viewport":{"width":100,"height":100}}"#).unwrap();
    let image = scenario
        .render_source(|(), clock| {
            let state = ImageCanvasState::default();
            state.set_document(
                Some(Arc::new(RasterImage::from_rgba(
                    image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 255, 0, 255])),
                ))),
                vec![],
            );
            plico_desktop::SurfaceSource::with_clock(
                CanvasView,
                ImageCanvas::new(state),
                "canvas",
                clock,
            )
        })
        .unwrap();
    assert!(image.rgba().as_chunks::<4>().0.contains(&[0, 255, 0, 255]));
}
