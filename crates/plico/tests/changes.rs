use plico::{Changes, Engine, Style, prelude::*};
#[test]
fn property_patch_is_generated_with_channels_and_handles_restoration() {
    use plico::StyleProperties as P;
    let base = Style::default();
    let changed = Style {
        height: Some(80.),
        opacity: 0.5,
        ..base
    };
    let diff = changed.diff(base);
    assert_eq!(diff.properties, P::HEIGHT | P::OPACITY);
    assert!(diff.changes.contains(Changes::LAYOUT | Changes::PAINT));
    assert_eq!(base.diff(changed).properties, diff.properties);
    assert!(base.diff(base).properties.is_empty());
    // An overridden padding input should not claim an effective change.
    let overridden = Style {
        padding_x: Some(7.),
        padding_y: Some(7.),
        ..base
    };
    assert!(
        Style {
            padding: 20.,
            ..overridden
        }
        .diff(overridden)
        .properties
        .is_empty()
    );
}
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

#[test]
fn shadow_changes_are_paint_only_and_do_not_alias_high_property_bits() {
    use plico::{BoxShadow, Color, StyleProperties as P};
    let base = Style::default();
    let changed = Style {
        shadow: Some(BoxShadow {
            offset_x: 0.,
            offset_y: 2.,
            blur: 12.,
            spread: -1.,
            color: Color::Literal(0),
            opacity: 0.08,
        }),
        ..base
    };
    assert!(changed.valid());
    let diff = changed.diff(base);
    assert_eq!(diff.changes, Changes::PAINT);
    assert_eq!(diff.properties, P::SHADOW);
    assert!(changed.layout_eq(base));
    assert!(!P::EXPANDED.intersects(P::SHADOW));
    assert!(P::EXPANDED.bits() > u64::MAX as u128);
    assert_eq!(base.diff(changed).properties, P::SHADOW);
}
