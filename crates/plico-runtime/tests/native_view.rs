use plico::{Theme, View, prelude::*};
use plico_runtime::{
    NativeView,
    testing::{Command, Driver},
};

#[derive(Default)]
struct Counter {
    count: u32,
}

impl View for Counter {
    fn theme(&self) -> Theme {
        Theme::default()
    }

    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        text!(cx, "Count: {}", self.count);
        if button(cx).text("Increment").clicked() {
            self.count += 1;
        }
    }
}

#[test]
fn native_view_adapts_the_minimal_application_contract() {
    let mut driver = Driver::new(NativeView(Counter::default()), 400, 300).unwrap();
    driver
        .execute(Command::Click {
            role: "button".into(),
            label: "Increment".into(),
            occurrence: None,
        })
        .unwrap();
    assert_eq!(driver.source.view.0.count, 1);
    assert!(
        driver
            .snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("Count: 1"))
    );
}
