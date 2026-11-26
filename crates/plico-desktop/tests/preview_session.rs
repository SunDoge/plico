#![cfg(feature = "wgpu")]
use plico::prelude::*;
use plico_desktop::{CaptureSession, HostView, PreviewScenario};

#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct Fixture {
    count: u32,
}
struct Demo {
    count: u32,
    now_ms: u128,
}
impl HostView for Demo {
    fn set_frame_time(&mut self, time: plico::FrameTime) {
        self.now_ms = time.now.as_millis();
    }
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        cx.request_animation_frame();
        column(cx)
            .width_percent(100.)
            .height_percent(100.)
            .background(Color::Literal(0xffffff))
            .children(|cx| {
                text!(cx, "Count {} / time {}", self.count, self.now_ms);
                if button(cx).text("Increment").clicked() {
                    self.count += 1;
                }
            });
    }
}
fn create(fixture: Fixture) -> Demo {
    Demo {
        count: fixture.count,
        now_ms: 0,
    }
}
fn recipe(index: usize) -> PreviewScenario<Fixture> {
    serde_json::from_value(serde_json::json!({
        "viewport": {"width": if index == 1 { 360 } else { 320 }, "height": 240,
                     "scale": if index == 2 { 1.5 } else { 1.0 }},
        "state": {"count": index * 10},
        "steps": [{"after_ms": index * 31, "command": {"action":"click", "role":"button", "label":"Increment"}}],
        "time_ms": 200
    })).unwrap()
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn shared_renderer_resets_app_input_clock_viewport_and_survives_recipe_errors() {
    let expected: Vec<_> = (0..3)
        .map(|index| recipe(index).render(create).unwrap())
        .collect();
    let mut session = CaptureSession::default();
    for index in [0, 1, 2, 2, 1, 0] {
        let actual = recipe(index).render_in(&mut session, create).unwrap();
        assert_eq!(actual, expected[index], "recipe {index}");
    }
    for invalid in [
        r#"{"version":2}"#,
        r#"{"viewport":{"width":0}}"#,
        r#"{"steps":[{"command":{"action":"click","role":"button","label":"missing"}}]}"#,
    ] {
        let bad: PreviewScenario<Fixture> = serde_json::from_str(invalid).unwrap();
        assert!(bad.render_in(&mut session, create).is_err());
        assert_eq!(
            recipe(0).render_in(&mut session, create).unwrap(),
            expected[0]
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn recording_samples_in_order_and_matches_the_exact_end_snapshot() {
    fn scenario() -> PreviewScenario<Fixture> {
        serde_json::from_value(serde_json::json!({
            "viewport": {"width": 320, "height": 240, "scale": 1.25},
            "steps": [
                {"after_ms": 35, "command": {"action":"click", "role":"button", "label":"Increment"}},
                {"after_ms": 0, "command": {"action":"click", "role":"button", "label":"Increment"}}
            ],
            "time_ms": 78
        })).unwrap()
    }
    let mut frames = Vec::new();
    scenario()
        .record_frames(20, create, |time, pixels| {
            frames.push((time.as_millis(), pixels));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        frames.iter().map(|(time, _)| *time).collect::<Vec<_>>(),
        [0, 50, 100, 113]
    );
    assert_eq!((frames[0].1.width(), frames[0].1.height()), (400, 300));
    assert_ne!(frames[0].1, frames[1].1);
    assert!(
        frames[3].1 == scenario().render(create).unwrap(),
        "last frame differs from the exact-time screenshot"
    );
}

#[test]
fn recording_rejects_invalid_fps_before_initializing_a_renderer() {
    let result = PreviewScenario::<Fixture>::default().record_frames(0, create, |_, _| {
        unreachable!("invalid FPS cannot emit frames")
    });
    assert!(matches!(
        result,
        Err(plico_desktop::PreviewError::RecordingFps { fps: 0 })
    ));
}
