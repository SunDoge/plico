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
                    .changed()
            );
        })
        .unwrap();
    assert_eq!(selected, 2);
    assert!(!popup.is_open());
}

#[test]
fn whitespace_and_unicode_queries_preserve_stable_prefix_priority() {
    let options = [
        SelectOption::new(1u64, 1, "Crème BRÛLÉE"),
        SelectOption::new(2u64, 2, "Brûlée A"),
        SelectOption::new(3u64, 3, "Crème brûlée B"),
        SelectOption::new(4u64, 4, "BRÛLÉE C"),
    ];
    let mut engine = Engine::default();
    let mut popup = PopoverState::default();
    popup.open();
    let query = TextBuffer::new();
    let mut selected = 0;
    for (input, order) in [("  bRûLéE ", [1, 3, 0, 2]), ("  ", [0, 1, 2, 3])] {
        query.set(input.to_owned().into());
        engine
            .frame(|cx| {
                plico::widgets::select(cx, &mut popup, "Dessert", &mut selected, &options)
                    .searchable(&query)
                    .motion(Motion::None);
            })
            .unwrap();
        let labels = ["Crème BRÛLÉE", "Brûlée A", "Crème brûlée B", "BRÛLÉE C"];
        let first = option(&engine, engine.root(), labels[order[0]]).unwrap();
        let parent = engine.parent(first).unwrap();
        let rows: Vec<_> = engine
            .node(parent)
            .unwrap()
            .children
            .iter()
            .filter_map(|id| {
                let node = engine.node(*id)?;
                (node.style.role == Some(plico::WidgetRole::Option)).then_some(node.text)
            })
            .collect();
        assert_eq!(rows, order.map(|index| labels[index]));
        assert_eq!(selected, 0);
    }
}
