#![cfg(feature = "wgpu")]
use plico::prelude::*;
use plico_desktop::{HostView, PreviewScenario, PreviewSession, RenderBackend};

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
        .map(|index| {
            recipe(index)
                .render_with_backend(RenderBackend::Wgpu, create)
                .unwrap()
        })
        .collect();
    let mut session = PreviewSession::new(RenderBackend::Wgpu);
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
