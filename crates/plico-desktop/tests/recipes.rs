#![cfg(feature = "wgpu")]
use plico::prelude::*;
use plico_desktop::{
    HostView, PreviewSession, RenderBackend,
    recipe::{RecipeMode, RecipePlan},
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

struct Demo {
    count: u64,
    now: u128,
    input: plico::TextBuffer,
    secret: plico::TextBuffer,
}
impl HostView for Demo {
    fn set_frame_time(&mut self, time: plico::FrameTime) {
        self.now = time.now.as_millis();
    }
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .width_percent(100.)
            .height_percent(100.)
            .background(Color::Literal(0xffffff))
            .children(|cx| {
                text!(cx, "Count {} time {}", self.count, self.now);
                if button(cx).text("Increment").clicked() {
                    self.count += 1;
                }
                text_input(cx, "Search", &self.input).width(220.);
                text_input(cx, "Password", &self.secret).width(220.);
            });
    }
}
fn create(state: Value) -> Demo {
    Demo {
        count: state["count"].as_u64().unwrap_or(0),
        now: 0,
        input: plico::TextBuffer::new(),
        secret: plico::TextBuffer::secret(),
    }
}
fn step(operation: Value) -> Value {
    json!({"source":{"file":"test.ts","line":12,"column":3},"flow":["shared-flow"],"operation":operation})
}
fn count(text: &str) -> Value {
    step(json!({"op":"assert","assertion":{"kind":"text","text":text,"present":true}}))
}
fn case(name: &str, steps: Vec<Value>) -> Value {
    json!({"name":name,"state":{},"viewport":{"width":320,"height":240,"scale":1},"steps":steps})
}
fn plan(cases: Vec<Value>) -> RecipePlan<Value> {
    serde_json::from_value(json!({"version":1,"cases":cases})).unwrap()
}
#[test]
fn semantic_plan_executes_in_order_injects_events_and_reports_failure_without_state_leaks() {
    let dir = tempfile::tempdir().unwrap();
    let p = plan(vec![
        case(
            "first",
            vec![
                step(
                    json!({"op":"command","command":{"action":"click","role":"button","label":"Increment"}}),
                ),
                count("Count 1 time 0"),
                step(json!({"op":"advance","milliseconds":150})),
                step(json!({"op":"event","event":{"count":7}})),
                count("Count 7 time 150"),
                step(json!({"op":"fill","label":"Search","text":"first"})),
                step(json!({"op":"fill","label":"Search","text":"replacement"})),
                step(
                    json!({"op":"assert","assertion":{"kind":"nodes","selector":{"role":"textbox","label":"Search"},"expect":{"kind":"value","value":"replacement"}}}),
                ),
                step(json!({"op":"fill","label":"Password","text":"never-in-evidence"})),
                count("intentional failure"),
                step(json!({"op":"capture","name":"not-reached"})),
            ],
        ),
        case("independent", vec![count("Count 0 time 0")]),
    ]);
    let results = p
        .run(
            &mut PreviewSession::new(RenderBackend::Wgpu),
            RecipeMode::Semantic,
            dir.path(),
            create,
            |view, value| {
                view.count = value["count"].as_u64().ok_or("expected count")?;
                Ok(())
            },
        )
        .unwrap();
    assert!(!results[0].passed);
    assert!(results[1].passed);
    let failure = results[0].steps.last().unwrap();
    assert_eq!(failure.index, 10);
    assert_eq!(failure.source.as_ref().unwrap().line, 12);
    assert_eq!(failure.flow, ["shared-flow"]);
    assert_eq!(failure.virtual_time_ms, 150);
    assert_eq!(results[0].steps.len(), 10);
    assert!(
        results[0]
            .evidence
            .iter()
            .any(|node| node.label.as_deref() == Some("Search")
                && node.value.as_deref() == Some("replacement"))
    );
    assert!(
        results[0]
            .evidence
            .iter()
            .any(|node| node.label.as_deref() == Some("Password") && node.value.is_none())
    );
    assert!(
        !serde_json::to_string(&results)
            .unwrap()
            .contains("never-in-evidence")
    );
    assert!(!dir.path().join("first/not-reached.png").exists());
}
#[test]
fn invalid_plan_never_calls_factory_or_creates_artifacts() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("not-created");
    let called = Rc::new(Cell::new(0));
    let mut p = plan(vec![case("duplicate", vec![]), case("duplicate", vec![])]);
    for iteration in 0..5 {
        match iteration {
            1 => {
                p.cases.pop();
                p.cases[0].name = "../escape".into();
            }
            2 => {
                p.cases[0].name = "okay".into();
                p.version = 2;
            }
            3 => {
                p.version = 1;
                p.cases[0].viewport.width = 0;
            }
            4 => {
                p.cases[0].viewport.width = 320;
                p.cases[0].steps = vec![
                    serde_json::from_value(step(json!({"op":"capture","name":"_failure"})))
                        .unwrap(),
                ];
            }
            _ => {}
        }
        assert!(p.validate().is_err());
    }
    let observed = called.clone();
    assert!(
        p.run(
            &mut PreviewSession::new(RenderBackend::Wgpu),
            RecipeMode::Semantic,
            &root,
            move |state| {
                observed.set(observed.get() + 1);
                create(state)
            },
            |_, _| Ok(())
        )
        .is_err()
    );
    assert_eq!(called.get(), 0);
    assert!(!root.exists());
    assert!(
        serde_json::from_value::<RecipePlan<Value>>(json!({"version":1,"cases":[],"typo":true}))
            .is_err()
    );
}
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn visual_checkpoints_capture_one_continuous_flow_and_keep_failure_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let p = plan(vec![
        case(
            "journey",
            vec![
                step(json!({"op":"capture","name":"initial"})),
                step(
                    json!({"op":"command","command":{"action":"click","role":"button","label":"Increment"}}),
                ),
                count("Count 1 time 0"),
                step(json!({"op":"capture","name":"incremented"})),
                count("intentional failure"),
            ],
        ),
        case(
            "fresh",
            vec![
                count("Count 0 time 0"),
                step(json!({"op":"capture","name":"initial"})),
            ],
        ),
    ]);
    let results = p
        .run(
            &mut PreviewSession::new(RenderBackend::Wgpu),
            RecipeMode::Visual,
            dir.path(),
            create,
            |_, _| Ok(()),
        )
        .unwrap();
    assert!(!results[0].passed);
    assert!(results[1].passed);
    assert_eq!(
        results[0].last_screenshot.as_deref(),
        Some("journey/incremented.png")
    );
    assert_eq!(
        results[0].failure_screenshot.as_deref(),
        Some("journey/_failure.png")
    );
    let initial = std::fs::read(dir.path().join("journey/initial.png")).unwrap();
    let changed = std::fs::read(dir.path().join("journey/incremented.png")).unwrap();
    assert_ne!(initial, changed);
    assert_eq!(
        initial,
        std::fs::read(dir.path().join("fresh/initial.png")).unwrap()
    );
    assert_eq!(
        changed,
        std::fs::read(dir.path().join("journey/_failure.png")).unwrap()
    );
}
