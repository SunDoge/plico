use plico::{Engine, Kind, Style, prelude::*};
#[test]
fn optimized_corners_are_the_default_and_explicit_circular_corners_remain_available() {
    assert_eq!(Style::default().corner_smoothing, 1.);
    assert_eq!(
        serde_json::from_str::<Style>("{}")
            .unwrap()
            .corner_smoothing,
        1.
    );
    let mut engine = Engine::default();
    let buffer = TextBuffer::default();
    let mut checked = false;
    engine
        .frame_declarations(|cx| {
            column(cx).radius(16.).children(|cx| {
                button(cx).text("Continue");
                text_input(cx, "Email", &buffer);
                checkbox(cx, "Remember", &mut checked);
                button(cx).text("Circular override").corner_smoothing(0.);
                button(cx).text("Circle").width(40).height(40).radius(20.);
            });
        })
        .unwrap();
    fn walk(engine: &Engine, id: plico::Entity, seen: &mut usize) {
        let node = engine.node(id).unwrap();
        if node.text == "Circular override" && matches!(node.kind, Kind::Button | Kind::ButtonBase)
        {
            assert_eq!(node.style.corner_smoothing, 0.);
        } else {
            assert_eq!(node.style.corner_smoothing, 1.);
        }
        if matches!(
            node.kind,
            Kind::Button | Kind::ButtonBase | Kind::TextInput | Kind::Checkbox
        ) {
            *seen += 1;
        }
        for &child in node.children {
            walk(engine, child, seen);
        }
    }
    let mut seen = 0;
    walk(&engine, engine.root(), &mut seen);
    assert!(seen >= 5);
}
