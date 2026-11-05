//! MyGO's field/form composition: label above the control, description and
//! danger-colored error below it, and fieldsets under a legend.
use plico::prelude::*;
use plico::{Engine, Entity};

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn find(engine: &Engine, label: &str) -> Option<Entity> {
    fn walk(engine: &Engine, id: Entity, label: &str, found: &mut Option<Entity>) {
        let node = engine.node(id).unwrap();
        if node.text == label && found.is_none() {
            *found = Some(id);
        }
        let children = node.children.to_vec();
        for child in children {
            walk(engine, child, label, found);
        }
    }
    let mut found = None;
    walk(engine, engine.root(), label, &mut found);
    found
}

#[test]
fn field_labels_the_control_and_shows_its_description_and_error() {
    let mut engine = Engine::default();
    let mut result = 0;
    frame(&mut engine, |cx| {
        result = field(cx, "Email")
            .description("For receipts.")
            .error("Enter an email address.")
            .children(|cx| {
                text(cx, "Control");
                7
            })
            .inner;
    });
    assert_eq!(result, 7, "the control callback's result is preserved");
    assert!(find(&engine, "Email").is_some());
    assert!(find(&engine, "Control").is_some());
    assert!(find(&engine, "For receipts.").is_some());
    let error = find(&engine, "Enter an email address.").expect("the error text shows");
    assert_eq!(
        engine.node(error).unwrap().style.foreground,
        Color::Danger,
        "errors use the danger color"
    );
}

#[test]
fn empty_description_and_error_declare_no_text() {
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        let _ = field(cx, "Email").description("").error("").children(|cx| {
            text(cx, "Control");
        });
    });
    assert!(find(&engine, "Email").is_some());
    let mut texts = Vec::new();
    fn collect(engine: &Engine, id: Entity, texts: &mut Vec<String>) {
        let node = engine.node(id).unwrap();
        if !node.text.is_empty() {
            texts.push(node.text.to_string());
        }
        let children = node.children.to_vec();
        for child in children {
            collect(engine, child, texts);
        }
    }
    collect(&engine, engine.root(), &mut texts);
    assert_eq!(texts, vec!["Email".to_string(), "Control".to_string()]);
}

#[test]
fn form_and_fieldset_group_fields_in_order() {
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        form(cx, |cx| {
            fieldset(cx, "Notifications", |cx| {
                let _ = field(cx, "Sound").children(|cx| {
                    let _ = switch(cx, "Sound", &mut true);
                });
                let _ = field(cx, "Badge").children(|cx| {
                    let _ = switch(cx, "Badge", &mut false);
                });
            });
        });
    });
    assert!(find(&engine, "Notifications").is_some());
    assert!(find(&engine, "Sound").is_some());
    assert!(find(&engine, "Badge").is_some());
}
