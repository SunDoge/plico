use plico_core::prelude::*;
use plico_core::{Engine, FrameError, Kind};

struct Custom<'a> {
    label: &'a str,
    amount: &'a mut usize,
}
impl Widget for Custom<'_> {
    type Response = usize;
    fn show(self, ui: &mut Ui) -> usize {
        ui.text(self.label);
        *self.amount += 1;
        *self.amount
    }
}
fn entity(engine: &Engine, label: &str) -> plico_core::Entity {
    *engine
        .node(engine.root())
        .unwrap()
        .children
        .iter()
        .find(|&&e| engine.node(e).unwrap().text == label)
        .unwrap()
}
#[component]
fn siblings(ui: &mut Ui, extra: bool, count: &mut usize) {
    if extra {
        ui.add(Custom {
            label: "optional",
            amount: count,
        });
    }
    let result = ui.add(Custom {
        label: "stable",
        amount: count,
    });
    assert!(result > 0);
}
#[test]
fn application_widget_is_statically_dispatched_and_keeps_identity_when_a_sibling_disappears() {
    let mut engine = Engine::default();
    let mut count = 0;
    engine
        .frame_declarations(|ui| siblings(ui, true, &mut count))
        .unwrap();
    let stable = entity(&engine, "stable");
    engine
        .frame_declarations(|ui| siblings(ui, false, &mut count))
        .unwrap();
    assert_eq!(entity(&engine, "stable"), stable);
    assert_eq!(count, 3);
}
#[test]
fn direct_show_and_plain_component_function_support_borrowed_state() {
    fn plain(ui: &mut Ui) -> u32 {
        ui.text("plain");
        7
    }
    let mut engine = Engine::default();
    let mut count = 0;
    #[component]
    fn show(ui: &mut Ui, count: &mut usize) {
        assert_eq!(
            Custom {
                label: "custom",
                amount: count
            }
            .show(ui),
            1
        );
        assert_eq!(ui.add(plain), 7);
    }
    engine
        .frame_declarations(|ui| show(ui, &mut count))
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn composite_content_inherits_primary_colors_and_does_not_leak_them_to_siblings() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|ui| {
            let result = ui.button("Save").primary().children(|ui| {
                ui.text("Save body");
                42
            });
            assert_eq!(result.inner, 42);
            assert!(result.response.enabled());
            ui.text("Outside");
        })
        .unwrap();
    let button = engine.node(entity(&engine, "Save")).unwrap();
    assert_eq!(button.kind, Kind::ButtonBase);
    assert_eq!(
        engine.node(button.children[0]).unwrap().style.foreground,
        Color::OnAccent
    );
    assert_eq!(
        engine
            .node(entity(&engine, "Outside"))
            .unwrap()
            .style
            .foreground,
        Color::Foreground
    );
}
#[test]
fn group_disabling_is_inherited_and_cannot_be_undone_by_a_child() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|ui| {
            let _ = ui.column().disabled(true).children(|ui| {
                let response = Button::new("Disabled group").disabled(false).show(ui);
                assert!(!response.enabled());
                assert!(!response.clicked());
            });
        })
        .unwrap();
    let group = engine.node(engine.root()).unwrap().children[0];
    assert!(
        engine
            .node(engine.node(group).unwrap().children[0])
            .unwrap()
            .style
            .disabled
    );
}
#[test]
fn nested_interactive_content_is_rejected_instead_of_adding_ambiguous_focus_targets() {
    let mut engine = Engine::default();
    let result = engine.frame_declarations(|ui| {
        ui.button_base("Outer").children(|ui| {
            ui.button("Inner");
        });
    });
    assert_eq!(result, Err(FrameError::NestedControl));
}
#[test]
fn legacy_theme_deserializes_and_metrics_are_shared_by_built_in_widgets() {
    let theme: Theme =
        serde_json::from_str(r#"{"surface":16777215,"foreground":0,"accent":10,"on_accent":0}"#)
            .unwrap();
    assert_eq!(theme.metrics, Metrics::default());
    let mut engine = Engine::default();
    let theme = Theme {
        metrics: Metrics::compact(),
        ..theme
    };
    engine.set_theme(theme);
    engine
        .frame_declarations(|ui| {
            Button::new("Compact").show(ui);
        })
        .unwrap();
    let style = engine.node(entity(&engine, "Compact")).unwrap().style;
    assert_eq!(style.font_size, theme.metrics.font_size);
    assert_eq!(
        style.horizontal_padding(),
        theme.metrics.spacing as f32 * 3.5
    );
    assert_eq!(style.vertical_padding(), theme.metrics.spacing as f32 * 1.5);
    assert_eq!(style.min_height, Some(theme.metrics.control_height as f32));
    assert_eq!(style.background, Some(Color::Control));
}
