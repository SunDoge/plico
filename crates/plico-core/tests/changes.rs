use plico_core::{Changes, Engine, Style, prelude::*};
#[test]
fn invalidation_is_specific_and_journal_is_per_node() {
    let old = Style::default();
    let transform = Style {
        translate: [30., 0.],
        ..old
    }
    .changes_from(old);
    assert!(
        transform
            .contains(Changes::TRANSFORM | Changes::PAINT | Changes::HIT_TEST | Changes::SEMANTICS)
    );
    assert!(!transform.intersects(Changes::LAYOUT | Changes::MEASURE));
    assert_eq!(
        Style {
            opacity: 0.5,
            ..old
        }
        .changes_from(old),
        Changes::PAINT
    );
    assert_eq!(
        Style {
            selected: Some(true),
            ..old
        }
        .changes_from(old),
        Changes::SEMANTICS
    );
    let font = Style {
        font_size: 24,
        ..old
    }
    .changes_from(old);
    assert!(font.contains(Changes::MEASURE | Changes::LAYOUT));
    assert!(
        !Style {
            scale: [f32::NAN, 1.],
            ..old
        }
        .valid()
    );
    let mut engine = Engine::default();
    for x in [0., 20., 20.] {
        engine
            .frame_declarations(|cx| {
                let _ = column(cx).translate(x, 0.).children(|cx| {
                    text(cx, "Stable");
                });
            })
            .unwrap();
        if x == 20. && !engine.node_changes().is_empty() {
            assert_eq!(engine.node_changes().len(), 1);
            assert!(
                engine.node_changes()[0]
                    .changes
                    .contains(Changes::TRANSFORM)
            );
            assert!(
                !engine
                    .changes()
                    .intersects(Changes::LAYOUT | Changes::MEASURE)
            );
        }
    }
    assert!(engine.node_changes().is_empty());
}
