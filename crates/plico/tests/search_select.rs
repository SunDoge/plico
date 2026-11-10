use plico::{Color, Engine, Entity, Kind, Motion, PopoverState, SelectOption, TextBuffer};
fn option(engine: &Engine, parent: Entity, text: &str) -> Option<Entity> {
    let node = engine.node(parent)?;
    if node.kind == Kind::ButtonBase && node.text == text {
        return Some(parent);
    }
    node.children
        .iter()
        .find_map(|id| option(engine, *id, text))
}
#[test]
fn search_filters_without_mutating_choice_and_selection_closes_popup() {
    let mut engine = Engine::default();
    engine.set_viewport(800., 600.).unwrap();
    let mut popup = PopoverState::default();
    popup.open();
    let query = TextBuffer::new();
    query.set("BE".to_owned().into());
    let mut selected = 99u64;
    let options = [
        SelectOption::new(1u64, 1u64, "Alpha"),
        SelectOption::new(2u64, 2u64, "Beta"),
    ];
    engine
        .frame(|cx| {
            plico::widgets::select(cx, &mut popup, "Model", &mut selected, &options)
                .searchable(&query)
                .motion(Motion::None);
        })
        .unwrap();
    assert_eq!(selected, 99);
    assert!(option(&engine, engine.root(), "Alpha").is_none());
    let beta = option(&engine, engine.root(), "Beta").expect("filtered option");
    let style = engine.node(beta).unwrap().style;
    assert_eq!(style.hover_background, Some(Color::Accent));
    assert_eq!(style.pressed_background, Some(Color::Accent));
    let trigger = option(&engine, engine.root(), "Model").expect("select trigger");
    assert!(engine.node(trigger).unwrap().style.styled_button);
    engine.queue_click(beta);
    engine
        .frame(|cx| {
            assert!(
                plico::widgets::select(cx, &mut popup, "Model", &mut selected, &options)
                    .searchable(&query)
                    .motion(Motion::None)
                    .response()
                    .changed
            );
        })
        .unwrap();
    assert_eq!(selected, 2);
    assert!(!popup.is_open());
}
