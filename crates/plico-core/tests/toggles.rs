use plico_core::{Engine, Entity, prelude::*};
fn named(engine: &Engine, label: &str) -> Entity {
    engine
        .node(engine.root())
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|id| engine.node(*id).unwrap().text == label)
        .unwrap()
}
#[component]
fn controls(cx: &mut UiContext, disabled: bool, value: &mut bool) -> ToggleResponse {
    checkbox(cx, "Choice", value)
        .disabled(disabled)
        .gap(10)
        .font_size(18)
        .motion(Motion::Smooth)
        .response()
}
#[test]
fn configured_toggle_returns_final_value_consumes_once_and_respects_disabled() {
    let mut engine = Engine::default();
    let mut value = false;
    let mut result = None;
    engine
        .frame_declarations(|cx| {
            result = Some(controls(cx, false, &mut value));
        })
        .unwrap();
    let id = named(&engine, "Choice");
    engine.queue_click(id);
    engine
        .frame_declarations(|cx| {
            result = Some(controls(cx, false, &mut value));
        })
        .unwrap();
    let response = result.unwrap();
    assert!(response.changed() && response.checked() && response.enabled());
    assert!(value && engine.node(id).unwrap().style.checked);
    let style = engine.node(id).unwrap().style;
    assert_eq!(
        (style.gap, style.font_size, style.transition_ms),
        (10., 18, 240)
    );
    engine
        .frame_declarations(|cx| {
            result = Some(controls(cx, false, &mut value));
        })
        .unwrap();
    assert!(!result.unwrap().changed());
    engine.queue_click(id);
    engine
        .frame_declarations(|cx| {
            result = Some(controls(cx, true, &mut value));
        })
        .unwrap();
    assert!(value && result.unwrap().checked());
    assert!(!result.unwrap().enabled() && !result.unwrap().changed());
    engine
        .frame_declarations(|cx| {
            result = Some(controls(cx, false, &mut value));
        })
        .unwrap();
    assert!(value && !result.unwrap().changed()); // disabled input did not linger
}
#[test]
fn dropping_a_toggle_submits_and_updates_its_borrowed_value() {
    let mut engine = Engine::default();
    let mut value = false;
    #[component]
    fn screen(cx: &mut UiContext, value: &mut bool) {
        switch(cx, "Switch", value).width(180.).padding(4);
    }
    engine
        .frame_declarations(|cx| screen(cx, &mut value))
        .unwrap();
    let id = named(&engine, "Switch");
    engine.queue_click(id);
    engine
        .frame_declarations(|cx| screen(cx, &mut value))
        .unwrap();
    assert!(value && engine.node(id).unwrap().style.checked);
    assert_eq!(engine.node(id).unwrap().style.width, Some(180.));
}
#[test]
fn generated_toggle_keys_survive_optional_siblings_and_reorder_uses_business_keys() {
    let mut engine = Engine::default();
    let mut values = [false; 3];
    #[component]
    fn screen(cx: &mut UiContext, extra: bool, reverse: bool, values: &mut [bool; 3]) {
        if extra {
            checkbox(cx, "optional", &mut values[0]);
        }
        switch(cx, "stable", &mut values[1]);
        for id in if reverse { [2_u64, 1] } else { [1, 2] } {
            cx.keyed(id, |cx| {
                checkbox(cx, if id == 1 { "one" } else { "two" }, &mut values[2]);
            });
        }
    }
    engine
        .frame_declarations(|cx| screen(cx, true, false, &mut values))
        .unwrap();
    let ids = [
        named(&engine, "stable"),
        named(&engine, "one"),
        named(&engine, "two"),
    ];
    engine.queue_click(ids[0]);
    engine
        .frame_declarations(|cx| screen(cx, false, true, &mut values))
        .unwrap();
    assert_eq!(
        ids,
        [
            named(&engine, "stable"),
            named(&engine, "one"),
            named(&engine, "two")
        ]
    );
    assert!(values[1]);
}
#[test]
fn disabled_ancestor_cannot_be_overridden_by_a_toggle() {
    let mut engine = Engine::default();
    let mut value = false;
    engine
        .frame_declarations(|cx| {
            let _ = column(cx).disabled(true).children(|cx| {
                let response = checkbox(cx, "Disabled", &mut value)
                    .disabled(false)
                    .response();
                assert!(!response.enabled());
            });
        })
        .unwrap();
    assert!(!value);
}
