use plico_core::{Engine, NumericRange, prelude::*};
#[test]
fn external_normalization_is_not_a_user_change_and_inherited_disabled_ignores_values() {
    let mut engine = Engine::default();
    let mut value = 7.3;
    let mut response = SliderResponse::default();
    #[component]
    fn form(cx: &mut UiContext, value: &mut f64, response: &mut SliderResponse, disabled: bool) {
        column(cx).disabled(disabled).children(|cx| {
            *response = slider(cx, "Volume", value, 0.0..=10.0).step(0.5).response();
        })
    }
    engine
        .frame_declarations(|cx| form(cx, &mut value, &mut response, false))
        .unwrap();
    assert_eq!(value, 7.5);
    assert!(!response.changed);
    let parent = engine.node(engine.root()).unwrap().children[0];
    let id = engine.node(parent).unwrap().children[0];
    engine.queue_value(id, 3.3);
    engine
        .frame_declarations(|cx| form(cx, &mut value, &mut response, true))
        .unwrap();
    assert_eq!(value, 7.5);
    assert!(!response.changed);
    engine.queue_value(id, 3.3);
    engine
        .frame_declarations(|cx| form(cx, &mut value, &mut response, false))
        .unwrap();
    assert_eq!(value, 3.5);
    assert!(response.changed);
    assert_eq!(engine.node(id).unwrap().numeric.unwrap().value, 3.5);
}
#[test]
fn range_endpoints_extreme_steps_and_invalid_configuration_are_defined() {
    let range = NumericRange {
        min: 0.,
        max: 10.,
        value: 0.,
        step: 3.,
        vertical: false,
        reversed: false,
    };
    assert_eq!(range.normalize(10.), 10.);
    assert_eq!(range.normalize(8.), 9.);
    assert_eq!(range.normalize(f64::INFINITY), 10.);
    assert_eq!(
        NumericRange {
            step: 1e-310,
            ..range
        }
        .normalize(5.),
        5.
    );
    assert_eq!(
        NumericRange {
            min: 2.,
            max: 2.,
            value: 2.,
            ..range
        }
        .fraction(),
        0.
    );
    let mut engine = Engine::default();
    let mut value = 1.;
    assert!(
        engine
            .frame_declarations(|cx| {
                slider(cx, "Invalid", &mut value, 0.0..=10.0).step(-1.);
            })
            .is_err()
    );
    assert!(
        engine
            .frame_declarations(|cx| {
                slider(cx, "Invalid", &mut value, f64::NAN..=10.0);
            })
            .is_err()
    );
}
