fn snapshot(value: &impl serde::Serialize) -> plico_dev::StateSnapshot {
    plico_dev::StateSnapshot::encode(value).unwrap()
}
use plico_core::{PathKey, UiContext};
use plico_dev::{App, Request, render};
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Counter {
    count: u64,
}
impl App for Counter {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        ui.column().key("app").children(|ui| {
            if ui.button("Increment").key("increment").clicked() {
                self.count += 1;
            }
            ui.text(format!("Count: {}", self.count)).key("count");
        });
    }
}
#[test]
fn logical_click_survives_recreated_arena_and_settles_once() {
    let frame = render::<Counter>(Request {
        state: snapshot(&serde_json::json!({"count":7})),
        clicks: vec![vec![
            PathKey::Named("app".into()),
            PathKey::Named("increment".into()),
        ]],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["count"],
        8
    );
    assert_eq!(frame.nodes[0].children[1].text, "Count: 8");
}
#[test]
fn incompatible_state_resets_and_reports_reason() {
    let frame = render::<Counter>(Request {
        state: snapshot(&serde_json::json!({"count":"old schema"})),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["count"],
        0
    );
    assert!(frame.reset.is_some());
}
#[test]
fn incompatible_protocol_is_rejected() {
    assert!(matches!(
        render::<Counter>(Request {
            version: 100,
            ..Default::default()
        }),
        Err(plico_dev::Error::Protocol { .. })
    ));
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Unstable;
impl App for Unstable {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        ui.request_rebuild();
    }
}
#[test]
fn unstable_view_is_an_error() {
    assert!(matches!(
        render::<Unstable>(Request::default()),
        Err(plico_dev::Error::DidNotSettle)
    ));
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Panics;
impl App for Panics {
    fn show(&mut self, _: &mut UiContext<'_>) {
        panic!("app failed")
    }
}
#[test]
fn panic_is_contained_inside_module_boundary() {
    let input = serde_json::to_vec(&Request::default()).unwrap();
    let reply = plico_dev::dispatch::<Panics>(input.into_iter().collect());
    let status = reply.status;
    let output = reply.bytes.to_vec();
    assert_ne!(status, 0);
    assert!(String::from_utf8(output).unwrap().contains("panicked"));
}

#[test]
fn logical_frame_time_and_animation_requests_cross_the_application_protocol() {
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Timer;
    impl App for Timer {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            ui.text(format!(
                "{} / {}",
                ui.now().as_millis(),
                ui.delta().as_millis()
            ));
            if ui.now() < std::time::Duration::from_secs(1) {
                ui.request_animation_frame();
            }
        }
    }
    let request = Request {
        time: plico_core::FrameTime {
            now: std::time::Duration::from_millis(100),
            delta: std::time::Duration::from_millis(16),
        },
        ..Default::default()
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    let reply = plico_dev::dispatch::<Timer>(bytes.into_iter().collect());
    assert_eq!(reply.status, 0);
    let frame: plico_dev::Frame = serde_json::from_slice(&reply.bytes).unwrap();
    assert_eq!(frame.nodes[0].text, "100 / 16");
    assert!(frame.requests.contains(plico_core::Changes::ANIMATE));
    let frame = render::<Timer>(Request {
        time: plico_core::FrameTime {
            now: std::time::Duration::from_secs(1),
            delta: std::time::Duration::from_millis(16),
        },
        ..Default::default()
    })
    .unwrap();
    assert!(!frame.requests.contains(plico_core::Changes::ANIMATE));
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Themed {
    compact: bool,
}
impl App for Themed {
    fn theme(&self) -> plico_core::Theme {
        let mut theme = plico_core::Theme::dark();
        if self.compact {
            theme.metrics = plico_core::Metrics::compact();
        }
        theme
    }
    #[plico_core::component]
    fn show(&mut self, ui: &mut UiContext) {
        ui.button("Composite").primary().children(|ui| {
            ui.text("Body");
        });
    }
}
#[test]
fn module_declarations_receive_app_theme_and_composite_content_survives_wire_round_trip() {
    let frame = render::<Themed>(Request {
        state: snapshot(&serde_json::json!({"compact":true})),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(frame.theme.metrics, plico_core::Metrics::compact());
    let button = &frame.nodes[0];
    assert_eq!(button.kind, plico_core::Kind::ButtonBase);
    assert_eq!(button.style.font_size, frame.theme.metrics.font_size);
    assert_eq!(
        button.style.min_height,
        Some(frame.theme.metrics.control_height as f32)
    );
    assert_eq!(
        button.children[0].style.foreground,
        plico_core::Color::OnAccent
    );
    let round_trip: plico_dev::Frame =
        serde_json::from_slice(&serde_json::to_vec(&frame).unwrap()).unwrap();
    assert_eq!(round_trip.nodes[0].children[0].text, "Body");
    assert_eq!(round_trip.theme, frame.theme);
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct ControlledToggle {
    checked: bool,
    locked: bool,
    actions: usize,
}
impl App for ControlledToggle {
    #[plico_core::component]
    fn show(&mut self, cx: &mut plico_core::UiContext) {
        if plico_core::widgets::checkbox(cx, "Controlled", &mut self.checked)
            .disabled(self.locked)
            .gap(10)
            .changed()
        {
            self.actions += 1;
        }
    }
}
#[test]
fn configured_toggle_crosses_module_protocol_without_retaining_a_business_borrow() {
    let first = render::<ControlledToggle>(Request::default()).unwrap();
    let path = vec![first.nodes[0].key.clone()];
    let changed = render::<ControlledToggle>(Request {
        state: first.state,
        clicks: vec![path.clone()],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        changed.state.decode::<serde_json::Value>().unwrap()["checked"],
        true
    );
    assert_eq!(
        changed.state.decode::<serde_json::Value>().unwrap()["actions"],
        1
    );
    assert!(changed.nodes[0].style.checked);
    assert_eq!(changed.nodes[0].style.gap, 10.);
    let mut state: serde_json::Value = changed.state.decode().unwrap();
    state["locked"] = true.into();
    let locked = render::<ControlledToggle>(Request {
        state: snapshot(&state),
        clicks: vec![path],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        locked.state.decode::<serde_json::Value>().unwrap()["checked"],
        true
    );
    assert_eq!(
        locked.state.decode::<serde_json::Value>().unwrap()["actions"],
        1
    );
    assert!(locked.nodes[0].style.disabled);
}

#[test]
fn modal_identity_dismissal_and_presence_survive_serialized_app_recreation() {
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Demo {
        state: plico_core::ModalState,
    }
    impl App for Demo {
        #[plico_core::component]
        fn show(&mut self, cx: &mut UiContext<'_>) {
            plico_core::widgets::column(cx).children(|cx| {
                plico_core::widgets::modal(cx, &mut self.state)
                    .title("Portable dialog")
                    .children(|cx| {
                        plico_core::widgets::text(cx, "Content");
                    });
            })
        }
    }
    let mut app = Demo::default();
    app.state.open();
    let opened = render::<Demo>(Request {
        state: snapshot(&app),
        ..Default::default()
    })
    .unwrap();
    let overlay = opened
        .nodes
        .iter()
        .find(|n| n.kind == plico_core::Kind::Overlay)
        .unwrap();
    let key = overlay.key.clone();
    let closing = render::<Demo>(Request {
        state: opened.state,
        clicks: vec![vec![key.clone()]],
        time: plico_core::FrameTime {
            now: std::time::Duration::from_millis(100),
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap();
    let overlay = closing
        .nodes
        .iter()
        .find(|n| n.kind == plico_core::Kind::Overlay)
        .unwrap();
    assert_eq!(overlay.key, key);
    assert!(!overlay.style.modal_open);
    assert_eq!(overlay.style.presence_until_ms, Some(240));
    let closed = render::<Demo>(Request {
        state: closing.state,
        time: plico_core::FrameTime {
            now: std::time::Duration::from_millis(250),
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap();
    assert!(
        closed
            .nodes
            .iter()
            .all(|n| n.kind != plico_core::Kind::Overlay)
    );
}

#[test]
fn numeric_events_replay_across_recreated_app_and_latest_value_wins() {
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Demo {
        value: f64,
    }
    impl App for Demo {
        fn show(&mut self, cx: &mut UiContext<'_>) {
            plico_core::widgets::slider(cx, "Volume", &mut self.value, 0.0..=10.0)
                .key("volume")
                .step(0.5);
        }
    }
    let frame = render::<Demo>(Request {
        values: vec![
            (vec![PathKey::Named("volume".into())], 1.),
            (vec![PathKey::Named("volume".into())], 7.3),
        ],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["value"],
        7.5
    );
    assert_eq!(frame.nodes[0].numeric.unwrap().value, 7.5);
    assert!(
        render::<Demo>(Request {
            state: frame.state,
            ..Default::default()
        })
        .is_ok()
    );
}

#[test]
fn virtual_list_uses_portable_scroll_feedback_after_module_reconstruction() {
    use plico_core::prelude::*;
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Demo;
    impl plico_dev::App for Demo {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            virtual_list(cx, 10_000, 40.).height(180.).rows(
                |i| i as u64,
                |cx, i| {
                    text!(cx, "Item {i}");
                },
            );
        }
    }
    let initial = plico_dev::render::<Demo>(plico_dev::Request::default()).unwrap();
    let path = vec![initial.nodes[0].key.clone()];
    let frame = plico_dev::render::<Demo>(plico_dev::Request {
        scroll_viewports: vec![(path, [4000., 180.])],
        ..Default::default()
    })
    .unwrap();
    let rows = &frame.nodes[0].children;
    assert_eq!(rows.len(), 11); // nine rows and two spacers
    assert_eq!(rows[1].children[0].text, "Item 98");
    assert_eq!(rows[9].children[0].text, "Item 106");
}

#[test]
fn request_without_optional_event_fields_deserializes() {
    // Headless scripts and older hosts may omit the event lists entirely.
    let request: Request = serde_json::from_str(r#"{"version":3,"state":[]}"#).unwrap();
    assert!(request.clicks.is_empty());
    assert!(request.values.is_empty());
    assert_eq!(request.version, plico_dev::PROTOCOL_VERSION);
}

#[test]
fn stale_path_events_are_counted_in_the_frame() {
    let frame = render::<Counter>(Request {
        clicks: vec![
            vec![
                PathKey::Named("app".into()),
                PathKey::Named("increment".into()),
            ],
            vec![PathKey::Named("app".into()), PathKey::Named("ghost".into())],
        ],
        ..Default::default()
    })
    .unwrap();
    // The real control consumed its click; the stale path is reported, not lost.
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["count"],
        1
    );
    assert_eq!(frame.unmatched_events, 1);
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Panicky;
impl App for Panicky {
    fn show(&mut self, _ui: &mut UiContext<'_>) {
        panic!("exploded during declaration");
    }
}

#[test]
fn panic_message_crosses_the_module_boundary() {
    // The panic payload must stay diagnosable after catch_unwind.
    let input: stabby::vec::Vec<u8> = serde_json::to_vec(&Request::default())
        .unwrap()
        .into_iter()
        .collect();
    let reply = plico_dev::dispatch::<Panicky>(input);
    let message = String::from_utf8(reply.bytes.into_iter().collect()).unwrap();
    assert!(message.contains("exploded during declaration"), "{message}");
}

#[test]
fn lean_frames_reuse_retained_state_without_roundtrip() {
    // Prime with a full frame: state 7 is deserialized and retained module-side.
    let frame = render::<Counter>(Request {
        state: snapshot(&serde_json::json!({"count":7})),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["count"],
        7
    );
    // A lean frame carries no state and returns none; the module's instance
    // keeps its count.
    let frame = render::<Counter>(Request {
        lean: true,
        ..Default::default()
    })
    .unwrap();
    assert!(frame.state.is_empty());
    // The next full frame serializes the retained instance: still 7.
    let frame = render::<Counter>(Request {
        lean: true,
        clicks: vec![vec![
            PathKey::Named("app".into()),
            PathKey::Named("increment".into()),
        ]],
        ..Default::default()
    })
    .unwrap();
    assert!(frame.state.is_empty());
    // Full frame: host sends its copy (7), module deserializes fresh, click
    // bumps it to 8, state returns.
    let frame = render::<Counter>(Request {
        state: snapshot(&serde_json::json!({"count":8})),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["count"],
        8
    );
}

#[test]
fn lean_frame_without_retained_instance_is_rejected() {
    // Fresh thread: no retained instance exists; a lean frame must not
    // silently reset to defaults.
    assert!(matches!(
        render::<Counter>(Request {
            lean: true,
            ..Default::default()
        }),
        Err(plico_dev::Error::Module { .. })
    ));
}

#[test]
fn error_frames_drop_retained_state_for_crash_safety() {
    // A full frame with a click primes the retained instance at count 1.
    let frame = render::<Counter>(Request {
        clicks: vec![vec![
            PathKey::Named("app".into()),
            PathKey::Named("increment".into()),
        ]],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["count"],
        1
    );
    // A failing frame of any type must drop the retained instance: a
    // subsequent lean frame errors instead of reusing possibly half-mutated
    // state, and the next full frame rebuilds from the host's state copy.
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Unstable2;
    impl App for Unstable2 {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            ui.request_rebuild();
        }
    }
    assert!(render::<Unstable2>(Request::default()).is_err());
    assert!(matches!(
        render::<Counter>(Request {
            lean: true,
            ..Default::default()
        }),
        Err(plico_dev::Error::Module { .. })
    ));
    // Recovery: a full frame rebuilds from host state.
    let frame = render::<Counter>(Request {
        state: snapshot(&serde_json::json!({"count":1})),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["count"],
        1
    );
    let frame = render::<Counter>(Request {
        lean: true,
        ..Default::default()
    })
    .unwrap();
    assert!(frame.state.is_empty());
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Editor {
    #[serde(skip)]
    buffer: plico_core::TextBuffer,
    echoed: String,
    submitted: bool,
}
impl App for Editor {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        let response = ui.text_input("Edit", &self.buffer).response();
        if response.changed {
            self.echoed = self.buffer.text().unwrap_or_default();
        }
        if response.submitted {
            self.submitted = true;
        }
    }
}

#[test]
fn native_input_content_crosses_the_module_boundary_by_revision() {
    // First render registers the buffer and reports empty content.
    let frame = render::<Editor>(Request::default()).unwrap();
    assert_eq!(frame.texts.len(), 1);
    let (path, content, _) = &frame.texts[0];
    assert_eq!(content.as_deref(), Some(""));
    // Host-side typing arrives as request texts; the module applies it to the
    // application's own buffer before declarations run.
    let frame = render::<Editor>(Request {
        texts: vec![(path.clone(), "hello".into(), 0)],
        ..Default::default()
    })
    .unwrap();
    let (_, content, _) = &frame.texts[0];
    assert_eq!(content.as_deref(), Some("hello"));
    // The application observed the change through its normal response path.
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["echoed"],
        "hello"
    );
    // Enter crosses as a submit counter.
    let frame = render::<Editor>(Request {
        texts: vec![(path.clone(), "hello".into(), 1)],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["submitted"],
        true
    );
    // A later frame repeating the same absolute count must not double-submit.
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Counting {
        #[serde(skip)]
        buffer: plico_core::TextBuffer,
        submits: u64,
    }
    impl App for Counting {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            if ui.text_input("Edit", &self.buffer).response().submitted {
                self.submits += 1;
            }
        }
    }
    let first = render::<Counting>(Request::default()).unwrap();
    let path = first.texts[0].0.clone();
    // The host threads state back on full frames, like ReloadView::dispatch.
    let second = render::<Counting>(Request {
        resume: true,
        state: first.state.clone(),
        texts: vec![(path.clone(), "a".into(), 2)],
        ..Default::default()
    })
    .unwrap();
    let repeated = render::<Counting>(Request {
        resume: true,
        state: second.state.clone(),
        texts: vec![(path.clone(), "a".into(), 2)],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        repeated.state.decode::<serde_json::Value>().unwrap()["submits"],
        1,
        "absolute submit counts apply once, not per frame"
    );
}

#[test]
fn resumed_event_keeps_mutations_from_lean_frames_and_checkpoint_is_read_only() {
    let initial = render::<Counter>(Request {
        state: snapshot(&serde_json::json!({"count":7})),
        ..Default::default()
    })
    .unwrap();
    render::<Counter>(Request {
        lean: true,
        clicks: vec![vec![
            PathKey::Named("app".into()),
            PathKey::Named("increment".into()),
        ]],
        ..Default::default()
    })
    .unwrap();
    let checkpoint = render::<Counter>(Request {
        checkpoint: true,
        resume: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        checkpoint.state.decode::<serde_json::Value>().unwrap()["count"],
        8
    );
    let event = render::<Counter>(Request {
        resume: true,
        state: initial.state,
        clicks: vec![vec![
            PathKey::Named("app".into()),
            PathKey::Named("increment".into()),
        ]],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        event.state.decode::<serde_json::Value>().unwrap()["count"],
        9,
        "host checkpoint must not overwrite a live session"
    );
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Form {
    #[serde(skip)]
    buffer: plico_core::TextBuffer,
    submits: u64,
    saved: String,
}
impl App for Form {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        if ui
            .text_input("Value", &self.buffer)
            .placeholder("Type here")
            .key("field")
            .response()
            .submitted
        {
            self.submits += 1;
        }
        if ui.button("Save").key("save").clicked() {
            self.saved = self.buffer.text().unwrap();
        }
    }
}
#[test]
fn resumed_inputs_survive_clicks_and_absolute_submissions_are_idempotent() {
    let initial = render::<Form>(Request::default()).unwrap();
    let path = initial.texts[0].0.clone();
    let entered = render::<Form>(Request {
        resume: true,
        texts: vec![(path.clone(), "hello".into(), 1)],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        entered.state.decode::<serde_json::Value>().unwrap()["submits"],
        1
    );
    let repeated = render::<Form>(Request {
        resume: true,
        state: entered.state,
        texts: vec![(path, "hello".into(), 1)],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        repeated.state.decode::<serde_json::Value>().unwrap()["submits"],
        1,
        "same submit must not trigger twice"
    );
    let saved = render::<Form>(Request {
        resume: true,
        state: repeated.state,
        clicks: vec![vec![PathKey::Named("save".into())]],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        saved.state.decode::<serde_json::Value>().unwrap()["saved"],
        "hello"
    );
    assert_eq!(saved.text_hints[0].1, "Type here");
}
#[test]
fn restoration_registers_inputs_before_clicks_and_does_not_replay_acknowledged_submit() {
    let path = vec![PathKey::Named("field".into())];
    let frame = render::<Form>(Request {
        state: snapshot(&serde_json::json!({"submits":3,"saved":"old"})),
        texts: vec![(path.clone(), "restored".into(), 4)],
        text_baselines: vec![(path, 3)],
        clicks: vec![vec![PathKey::Named("save".into())]],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["saved"],
        "restored",
        "click cannot run before text restoration"
    );
    assert_eq!(
        frame.state.decode::<serde_json::Value>().unwrap()["submits"],
        4,
        "only the pending submit is delivered"
    );
}

#[test]
fn checkpoint_does_not_run_declarations_and_old_protocol_is_rejected() {
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Shows {
        count: u64,
    }
    impl App for Shows {
        fn show(&mut self, _: &mut UiContext<'_>) {
            self.count += 1;
        }
    }
    assert_eq!(
        render::<Shows>(Request::default())
            .unwrap()
            .state
            .decode::<serde_json::Value>()
            .unwrap()["count"],
        1
    );
    for _ in 0..2 {
        assert_eq!(
            render::<Shows>(Request {
                checkpoint: true,
                ..Default::default()
            })
            .unwrap()
            .state
            .decode::<serde_json::Value>()
            .unwrap()["count"],
            1
        );
    }
    assert!(matches!(
        render::<Shows>(Request {
            version: 1,
            ..Default::default()
        }),
        Err(plico_dev::Error::Protocol { actual: 1 })
    ));
}
