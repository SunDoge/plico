use plico::{Capabilities as C, Kind};

#[test]
fn activation_editing_selection_and_focus_are_independent() {
    const TEXT: C = Kind::Text.capabilities();
    assert!(TEXT.contains(C::TEXT_SELECT));
    assert!(!Kind::Text.interactive());
    assert!(!TEXT.intersects(C::ACTIVATE | C::FOCUS));
    let editor = Kind::TextInput.capabilities();
    assert!(editor.contains(C::TEXT_EDIT | C::TEXT_SELECT | C::FOCUS | C::TAB_STOP));
    assert!(!editor.contains(C::ACTIVATE));
    let slider = Kind::Slider.capabilities();
    assert!(slider.contains(C::VALUE_EDIT | C::FOCUS | C::TAB_STOP));
    assert!(!slider.intersects(C::ACTIVATE | C::TEXT_EDIT));
    assert!(
        Kind::ButtonBase
            .capabilities()
            .contains(C::ACTIVATE | C::FOCUS)
    );
    assert!(Kind::Overlay.capabilities().contains(C::ACTIVATE));
    assert!(!Kind::Overlay.capabilities().contains(C::FOCUS));
    assert!(Kind::Dialog.capabilities().contains(C::FOCUS));
    assert!(!Kind::Dialog.capabilities().contains(C::TAB_STOP));
}

#[test]
fn every_tab_stop_can_receive_focus_and_states_do_not_change_capabilities() {
    for kind in [
        Kind::Root,
        Kind::Column,
        Kind::Row,
        Kind::Overlay,
        Kind::Dialog,
        Kind::TooltipAnchor,
        Kind::Text,
        Kind::TextInput,
        Kind::Button,
        Kind::ButtonBase,
        Kind::Checkbox,
        Kind::Switch,
        Kind::Slider,
        Kind::Svg,
        Kind::Shader,
    ] {
        let caps = kind.capabilities();
        assert!(!caps.contains(C::TAB_STOP) || caps.contains(C::FOCUS));
    }
}

#[test]
fn changing_kind_marks_structure_without_replacing_the_entity() {
    use plico::{Changes, Engine, Key, Style};
    let mut engine = Engine::default();
    let declare = |engine: &mut Engine, kind| {
        engine
            .frame_declarations(|cx| {
                cx.element(Key::Static("slot"), kind, Style::default(), "same", |_| {});
            })
            .unwrap();
    };
    declare(&mut engine, Kind::Text);
    let entity = engine.node(engine.root()).unwrap().children[0];
    engine.take_frame_delta();
    declare(&mut engine, Kind::ButtonBase);
    let delta = engine.take_frame_delta();
    assert!(engine.changes().contains(Changes::STRUCTURE));
    assert!(delta.removed.is_empty());
    assert_eq!(engine.node(engine.root()).unwrap().children, &[entity]);
}
