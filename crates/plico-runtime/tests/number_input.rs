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
    assert_eq!(ui.view().value, 5.5);
    assert_eq!(field_value(&ui), "5.5");

    ui.execute(Command::Text { text: "x".into() }).unwrap();
    assert_eq!(field_value(&ui), "5.5x");
    assert_eq!(ui.view().value, 5.5);
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
    assert_eq!(ui.view().value, 0.);
    assert_eq!(field_value(&ui), "0");
}

#[derive(Clone, Copy, Debug)]
enum Edit {
    Up,
    Decrease,
    AppendInvalid,
    Blur,
}

#[test]
fn short_edit_sequences_match_the_number_field_contract() {
    use Edit::*;
    let edits = [Up, Decrease, AppendInvalid, Blur];
    for initial in [0.5, 9.5] {
        for first in edits {
            for second in edits {
                for third in edits {
                    check_sequence(initial, [first, second, third]);
                }
            }
        }
    }
}

fn check_sequence(initial: f64, sequence: [Edit; 3]) {
    let mut ui = Driver::new(NumberField { value: initial }, 360, 180).unwrap();
    let mut value = initial;
    let mut draft = value.to_string();
    for (step, edit) in sequence.iter().enumerate() {
        let context = || format!("initial {initial}, actions {:?}", &sequence[..=step]);
        let result = match edit {
            Edit::Up | Edit::AppendInvalid => {
                ui.execute(Command::Focus {
                    role: "textbox".into(),
                    label: "Count".into(),
                    occurrence: None,
                })
                .unwrap_or_else(|e| panic!("{}: {e}", context()));
                if matches!(edit, Edit::Up) {
                    value = (value + 0.5).min(10.);
                    draft = value.to_string();
                    ui.key("ArrowUp")
                } else {
                    ui.key("End")
                        .unwrap_or_else(|e| panic!("{}: {e}", context()));
                    draft.push('x');
                    ui.type_text("x")
                }
            }
            Edit::Decrease if value > 0. => {
                value -= 0.5;
                draft = value.to_string();
                ui.click("Decrease Count")
            }
            Edit::Decrease => {
                // The contract offers no decrease action at the lower bound.
                assert!(ui.find("Decrease Count").unwrap().disabled, "{}", context());
                continue;
            }
            Edit::Blur => {
                draft = value.to_string();
                ui.click("Outside")
            }
        };
        result.unwrap_or_else(|e| panic!("{}: {e}", context()));
        assert_eq!(ui.view().value, value, "{}: bound value", context());
        assert_eq!(field_value(&ui), draft, "{}: displayed draft", context());
        assert_eq!(
            ui.find("Decrease Count").unwrap().disabled,
            value == 0.,
            "{}: decrease availability",
            context()
        );
        assert_eq!(
            ui.find("Increase Count").unwrap().disabled,
            value == 10.,
            "{}: increase availability",
            context()
        );
    }
}
