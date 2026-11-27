use plico::prelude::*;
use plico_renderer::SemanticRole;
use plico_runtime::{
    HostView,
    testing::{Command, DifferentialTester, Error, Tester},
};
use std::time::Duration;

#[derive(Default)]
struct Counter {
    count: u32,
}
impl HostView for Counter {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        text!(cx, "Count: {}", self.count);
        if button(cx).text("Increment").clicked() {
            self.count += 1;
        }
        if button(cx).text("Reset").clicked() {
            self.count = 0;
        }
        button(cx).text("Disabled").disabled(true);
        button(cx).text("Repeated");
        button(cx).text("Repeated");
    }
}
fn target(label: &str) -> Command {
    Command::Focus {
        role: "button".into(),
        label: label.into(),
        occurrence: None,
    }
}
#[test]
fn semantic_commands_drive_real_keyboard_input_without_a_window() {
    let mut driver = Tester::new(Counter::default(), 400, 300).unwrap();
    let click =
        serde_json::from_str(r#"{"action":"click","role":"button","label":"Increment"}"#).unwrap();
    driver.execute(click).unwrap();
    assert_eq!(driver.view().count, 1);
    driver.resize(1000, 640).unwrap();
    driver.execute(target("Increment")).unwrap();
    driver
        .execute(Command::Key {
            key: "Enter".into(),
        })
        .unwrap();
    assert_eq!(driver.view().count, 2);
    driver.execute(Command::Key { key: "Tab".into() }).unwrap();
    driver
        .execute(Command::Key {
            key: "Enter".into(),
        })
        .unwrap();
    assert_eq!(driver.view().count, 0);
    driver.update_view(|view| view.count = 7).unwrap();
    assert_eq!(driver.view().count, 7);
    driver.advance(Duration::from_secs(1)).unwrap();
    assert!(
        driver
            .snapshot()
            .nodes
            .iter()
            .any(|n| n.label.as_deref() == Some("Count: 7"))
    );
}

#[test]
fn semantic_queries_and_pointer_clicks_use_the_real_hit_test() {
    let mut driver = Tester::new(Counter::default(), 400, 300).unwrap();
    assert_eq!(
        driver
            .get_by_role(SemanticRole::Button, "Increment")
            .unwrap()
            .label
            .as_deref(),
        Some("Increment")
    );
    assert_eq!(driver.all_by_role(SemanticRole::Button).len(), 5);
    assert!(driver.unnamed_controls().is_empty());
    driver
        .execute(Command::PointerClick {
            role: "button".into(),
            label: "Increment".into(),
            occurrence: None,
        })
        .unwrap();
    assert_eq!(driver.view().count, 1);
}

#[test]
fn tester_offers_mygo_style_name_based_interactions() {
    let mut ui = Tester::new(Counter::default(), 400, 300).unwrap();
    assert!(ui.has_text("Count:"));
    assert!(ui.texts().iter().any(|text| text == "Increment"));
    assert_eq!(ui.find("Increment").unwrap().role, SemanticRole::Button);

    ui.move_to("Increment").unwrap();
    ui.click("Increment").unwrap();
    assert_eq!(ui.view().count, 1);

    ui.key("Tab").unwrap();
    assert!(matches!(
        ui.find("Repeated"),
        Err(Error::Target { matches: 2, .. })
    ));
}

#[test]
fn tester_drag_by_uses_pressed_pointer_frames() {
    #[derive(Default)]
    struct Draggable {
        offset: (f64, f64),
    }
    impl HostView for Draggable {
        fn show(&mut self, cx: &mut UiContext) {
            let mut delta = (0., 0.);
            drag_area_content(
                cx,
                "Move tile",
                |cx| {
                    text(cx, "Tile");
                },
                &mut |x, y| delta = (x, y),
            );
            self.offset.0 += delta.0;
            self.offset.1 += delta.1;
        }
    }

    let mut ui = Tester::new(Draggable::default(), 400, 300).unwrap();
    ui.drag_by("Move tile", 24., 12.).unwrap();
    assert_eq!(ui.view().offset, (24., 12.));
}

#[test]
fn differential_tester_matches_cold_replay_across_structure_motion_and_resize() {
    #[derive(Clone, Default)]
    struct Changing {
        rows: usize,
        expanded: bool,
        animated_width: f32,
    }
    impl HostView for Changing {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).gap(8).width_percent(100.)
                + |cx| {
                    if button(cx).text("Add row").clicked() {
                        self.rows += 1;
                    }
                    if button(cx).text("Toggle panel").clicked() {
                        self.expanded = !self.expanded;
                    }
                    for row in 0..self.rows {
                        cx.keyed(row, |cx| {
                            text!(cx, "Row {}", row + 1);
                        });
                    }
                    self.animated_width = cx.animate(
                        "panel-width",
                        if self.expanded { 180. } else { 80. },
                        Duration::from_millis(120),
                    );
                    column(cx)
                        .width(self.animated_width)
                        .height(24.)
                        .background(Color::Accent)
                        .children(|_| {});
                };
        }
    }

    let mut ui = DifferentialTester::new(Changing::default(), 400, 300).unwrap();
    ui.execute(Command::PointerClick {
        role: "button".into(),
        label: "Add row".into(),
        occurrence: None,
    })
    .unwrap();
    ui.execute(Command::PointerClick {
        role: "button".into(),
        label: "Toggle panel".into(),
        occurrence: None,
    })
    .unwrap();
    ui.advance(Duration::from_millis(48)).unwrap();
    ui.resize(640, 360).unwrap();
    ui.settle().unwrap();

    assert_eq!(ui.view().rows, 1);
    assert_eq!(ui.view().animated_width, 180.);
    assert!(
        ui.snapshot()
            .nodes
            .iter()
            .any(|node| { node.label.as_deref() == Some("Row 1") })
    );
}

#[test]
fn differential_tester_reports_nondeterministic_declarations() {
    use std::{cell::Cell, rc::Rc};

    #[derive(Clone)]
    struct Nondeterministic(Rc<Cell<usize>>);
    impl HostView for Nondeterministic {
        fn show(&mut self, cx: &mut UiContext) {
            let value = self.0.get() + 1;
            self.0.set(value);
            text!(cx, "Build {value}");
        }
    }

    assert!(matches!(
        DifferentialTester::new(Nondeterministic(Rc::new(Cell::new(0))), 200, 100),
        Err(Error::Mismatch {
            surface,
            ..
        }) if surface == "declarations"
    ));
}

#[test]
fn pointer_click_reports_when_another_control_covers_the_target() {
    #[derive(Default)]
    struct Covered {
        back: usize,
        front: usize,
    }
    impl HostView for Covered {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .position(Position::Relative)
                .width(200.)
                .height(100.)
                + |cx| {
                    if button(cx)
                        .text("Back")
                        .absolute(0., 0.)
                        .width(120.)
                        .height(60.)
                        .clicked()
                    {
                        self.back += 1;
                    }
                    if button(cx)
                        .text("Front")
                        .absolute(0., 0.)
                        .width(120.)
                        .height(60.)
                        .clicked()
                    {
                        self.front += 1;
                    }
                };
        }
    }
    let mut driver = Tester::new(Covered::default(), 300, 200).unwrap();
    assert!(matches!(
        driver.execute(Command::PointerClick {
            role: "button".into(),
            label: "Back".into(),
            occurrence: None,
        }),
        Err(Error::PointerTarget { hit: Some(_), .. })
    ));
    driver
        .execute(Command::PointerClick {
            role: "button".into(),
            label: "Front".into(),
            occurrence: None,
        })
        .unwrap();
    assert_eq!(driver.view().front, 1);
    assert_eq!(driver.view().back, 0);
}

#[test]
fn settle_advances_animations_without_sleeping() {
    #[derive(Default)]
    struct Animated {
        expanded: bool,
        value: f32,
    }
    impl HostView for Animated {
        fn show(&mut self, cx: &mut UiContext) {
            self.value = cx.animate(
                "width",
                if self.expanded { 100. } else { 0. },
                Duration::from_millis(100),
            );
            text!(cx, "{:.0}", self.value);
        }
    }
    let mut driver = Tester::new(Animated::default(), 300, 200).unwrap();
    driver.update_view(|view| view.expanded = true).unwrap();
    assert!(driver.frame_pending());
    driver.settle().unwrap();
    assert!(!driver.frame_pending());
    assert_eq!(driver.view().value, 100.);
}
#[test]
fn bad_targets_are_diagnostic_instead_of_silent_noops() {
    let mut driver = Tester::new(Counter::default(), 400, 300).unwrap();
    assert!(matches!(
        driver.execute(target("Missing")),
        Err(Error::Target { matches: 0, .. })
    ));
    assert!(matches!(
        driver.execute(target("Repeated")),
        Err(Error::Target { matches: 2, .. })
    ));
    driver
        .execute(Command::Focus {
            role: "button".into(),
            label: "Repeated".into(),
            occurrence: Some(1),
        })
        .unwrap();
    assert!(matches!(
        driver.execute(target("Disabled")),
        Err(Error::Rejected { .. })
    ));
    assert!(matches!(driver.resize(0, 100), Err(Error::Viewport)));
}

#[test]
fn json_pointer_commands_redeclare_and_clear_the_pointer_snapshot() {
    #[derive(Default)]
    struct Probe {
        seen: Option<(f64, f64)>,
    }
    impl HostView for Probe {
        fn show(&mut self, cx: &mut UiContext) {
            self.seen = cx.pointer_position();
            text(cx, "Pointer probe");
        }
    }
    let mut driver = Tester::new(Probe::default(), 1120, 820).unwrap();
    driver
        .execute(serde_json::from_str(r#"{"action":"pointer_move","x":675,"y":480}"#).unwrap())
        .unwrap();
    assert_eq!(driver.view().seen, Some((675., 480.)));
    assert!(matches!(
        driver.execute(Command::PointerMove { x: f64::NAN, y: 0. }),
        Err(Error::PointerPosition)
    ));
    assert_eq!(driver.view().seen, Some((675., 480.)));
    driver
        .execute(serde_json::from_str(r#"{"action":"pointer_leave"}"#).unwrap())
        .unwrap();
    assert_eq!(driver.view().seen, None);
}

#[test]
fn retained_frames_refresh_pointer_readers_but_leave_other_views_idle() {
    use plico_renderer::{Scene, TextContext};
    use plico_runtime::{Source, testing::dispatch};
    #[derive(Default)]
    struct Probe {
        read: bool,
        shows: usize,
        seen: Option<(f64, f64)>,
    }
    impl HostView for Probe {
        fn show(&mut self, cx: &mut UiContext) {
            self.shows += 1;
            if self.read {
                self.seen = cx.pointer_position();
            }
            text(cx, "Pointer observer");
        }
    }
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        Probe {
            read: true,
            ..Default::default()
        },
        clock,
    );
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_frame(&mut scene, &mut text, (1120, 820), 1.)
        .unwrap();
    dispatch(
        &mut source,
        &mut text,
        &Command::PointerMove { x: 675., y: 480. },
    )
    .unwrap();
    source
        .render_frame(&mut scene, &mut text, (1120, 820), 1.)
        .unwrap();
    assert_eq!(source.view().seen, Some((675., 480.)));
    dispatch(&mut source, &mut text, &Command::PointerLeave).unwrap();
    source
        .render_frame(&mut scene, &mut text, (1120, 820), 1.)
        .unwrap();
    assert_eq!(source.view().seen, None);
    source.update(|view| view.read = false);
    source
        .render_frame(&mut scene, &mut text, (1120, 820), 1.)
        .unwrap();
    let shows = source.view().shows;
    dispatch(
        &mut source,
        &mut text,
        &Command::PointerMove { x: 900., y: 750. },
    )
    .unwrap();
    source
        .render_frame(&mut scene, &mut text, (1120, 820), 1.)
        .unwrap();
    assert_eq!(
        source.view().shows,
        shows,
        "unobserved pointer movement should not redeclare"
    );
}
