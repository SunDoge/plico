use plico::prelude::*;
use plico_runtime::{
    View,
    testing::{Command, Driver, Error},
};
use std::time::Duration;

#[derive(Default)]
struct Counter {
    count: u32,
}
impl View for Counter {
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
    assert_eq!(driver.source.view.count, 1);
    driver.resize(1000, 640).unwrap();
    driver.execute(target("Increment")).unwrap();
    driver
        .execute(Command::Key {
            key: "Enter".into(),
        })
        .unwrap();
    assert_eq!(driver.source.view.count, 2);
    driver.execute(Command::Key { key: "Tab".into() }).unwrap();
    driver
        .execute(Command::Key {
            key: "Enter".into(),
        })
        .unwrap();
    assert_eq!(driver.source.view.count, 0);
    driver.advance(Duration::from_secs(1)).unwrap();
    assert!(
        driver
            .snapshot()
            .nodes
            .iter()
            .any(|n| n.label.as_deref() == Some("Count: 0"))
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
