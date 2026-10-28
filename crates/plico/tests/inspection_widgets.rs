use plico::{Engine, Entity, prelude::*};
fn find(e: &Engine, id: Entity, label: &str) -> Option<Entity> {
    let n = e.node(id)?;
    if n.text == label {
        return Some(id);
    }
    n.children.iter().find_map(|&c| find(e, c, label))
}
#[test]
fn rows_retain_business_identity_and_inherit_disabled() {
    let mut e = Engine::default();
    let mut selected = false;
    let mut frame = |e: &mut Engine, disabled| {
        e.frame_declarations(|cx| {
            column(cx).disabled(disabled).children(|cx| {
                let cols = [TableColumn::new("Name", 2.), TableColumn::new("State", 1.)];
                table_header(cx, &cols);
                cx.keyed("file-123", |cx| {
                    if table_row(cx, "Choose file", selected, &cols, &["auth.rs", "Ready"])
                        .clicked()
                    {
                        selected = !selected;
                    }
                });
                property_field(cx, "Object ID", &"a".repeat(64));
            });
        })
        .unwrap();
    };
    frame(&mut e, false);
    let id = find(&e, e.root(), "Choose file").unwrap();
    e.queue_click(id);
    frame(&mut e, false);
    assert_eq!(find(&e, e.root(), "Choose file"), Some(id));
    assert_eq!(e.node(id).unwrap().style.selected, Some(false));
    frame(&mut e, false);
    assert_eq!(e.node(id).unwrap().style.selected, Some(true));
    e.queue_click(id);
    frame(&mut e, true);
    frame(&mut e, false);
    assert_eq!(e.node(id).unwrap().style.selected, Some(true));
}
#[test]
fn empty_values_and_invalid_column_weights_are_safe() {
    let mut e = Engine::default();
    e.frame_declarations(|cx| {
        let cols = [
            TableColumn::new("Bad", f32::NAN),
            TableColumn::new("Zero", 0.),
        ];
        table_header(cx, &cols);
        table_row(cx, "Row", false, &cols, &["One"]);
        property_field(cx, "Empty", "");
    })
    .unwrap();
    assert!(find(&e, e.root(), "—").is_some());
}
