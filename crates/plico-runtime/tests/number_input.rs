use plico::prelude::*;
use plico_runtime::{
    HostView,
    testing::{Command, Driver},
};

struct NumberField {
    value: f64,
}

impl HostView for NumberField {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).children(|cx| {
            number_input(cx, "Count", &mut self.value, 0., 10., 0.5);
            button(cx).text("Outside");
        });
    }
}

fn field_value(ui: &Driver<NumberField>) -> String {
    ui.snapshot()
        .exposed_nodes()
        .into_iter()
        .find(|node| node.role.as_str() == "textbox" && node.label.as_deref() == Some("Count"))
        .and_then(|node| node.value.clone())
        .expect("number field value")
}

#[test]
fn number_input_routes_arrows_and_canonicalizes_when_focus_leaves() {
    let mut ui = Driver::new(NumberField { value: 5. }, 360, 180).unwrap();
    ui.execute(Command::Focus {
        role: "textbox".into(),
        label: "Count".into(),
        occurrence: None,
    })
    .unwrap();
    ui.execute(Command::Key {
        key: "ArrowUp".into(),
    })
    .unwrap();
    assert_eq!(ui.source.view.value, 5.5);
    assert_eq!(field_value(&ui), "5.5");

    ui.execute(Command::Text { text: "x".into() }).unwrap();
    assert_eq!(field_value(&ui), "5.5x");
    assert_eq!(ui.source.view.value, 5.5);
    ui.execute(Command::Focus {
        role: "button".into(),
        label: "Outside".into(),
        occurrence: None,
    })
    .unwrap();
    assert_eq!(field_value(&ui), "5.5");

    ui.execute(Command::Focus {
        role: "textbox".into(),
        label: "Count".into(),
        occurrence: None,
    })
    .unwrap();
    for _ in 0..20 {
        ui.execute(Command::Key {
            key: "ArrowDown".into(),
        })
        .unwrap();
    }
    assert_eq!(ui.source.view.value, 0.);
    assert_eq!(field_value(&ui), "0");
}
