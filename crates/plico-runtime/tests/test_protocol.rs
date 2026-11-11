use plico::prelude::*;
use plico_runtime::{
    HostView,
    testing::{Command, Driver, Error},
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
    let mut driver = Driver::new(Counter::default(), 400, 300).unwrap();
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
fn bad_targets_are_diagnostic_instead_of_silent_noops() {
    let mut driver = Driver::new(Counter::default(), 400, 300).unwrap();
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
    let mut driver = Driver::new(Probe::default(), 1120, 820).unwrap();
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
    use plico_renderer::{FrameSource, Scene, TextContext};
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
    source.render_retained_frame(&mut scene, &mut text, (1120, 820), 1.);
    dispatch(
        &mut source,
        &mut text,
        &Command::PointerMove { x: 675., y: 480. },
    )
    .unwrap();
    source.render_retained_frame(&mut scene, &mut text, (1120, 820), 1.);
    assert_eq!(source.view.seen, Some((675., 480.)));
    dispatch(&mut source, &mut text, &Command::PointerLeave).unwrap();
    source.render_retained_frame(&mut scene, &mut text, (1120, 820), 1.);
    assert_eq!(source.view.seen, None);
    source.view.read = false;
    source.request_rebuild();
    source.render_retained_frame(&mut scene, &mut text, (1120, 820), 1.);
    let shows = source.view.shows;
    dispatch(
        &mut source,
        &mut text,
        &Command::PointerMove { x: 900., y: 750. },
    )
    .unwrap();
    source.render_retained_frame(&mut scene, &mut text, (1120, 820), 1.);
    assert_eq!(
        source.view.shows, shows,
        "unobserved pointer movement should not redeclare"
    );
}
