use plico::{BoxShadow, Engine, Kind, Style, prelude::*};

fn shadow() -> BoxShadow {
    BoxShadow {
        offset_x: 0.,
        offset_y: 2.,
        blur: 8.,
        spread: 1.,
        color: Color::Foreground,
        opacity: 0.12,
    }
}
#[test]
fn inset_is_opt_in_validated_and_clearable_on_controls_and_layouts() {
    assert!(Style::default().inset_shadow.is_none());
    assert!(
        !Style {
            inset_shadow: Some(BoxShadow {
                blur: f32::NAN,
                ..shadow()
            }),
            ..Default::default()
        }
        .valid()
    );
    assert_eq!(
        serde_json::from_str::<Style>("{}").unwrap().inset_shadow,
        None
    );
    let mut engine = Engine::default();
    let declare = |cx: &mut UiContext<'_>, clear: bool| {
        let layout = column(cx).inset_shadow(shadow());
        let layout = if clear { layout.no_shadow() } else { layout };
        layout.children(|cx| {
            let control = button(cx).text("Inset").inset_shadow(shadow());
            let _ = if clear { control.no_shadow() } else { control };
        });
    };
    engine.frame_declarations(|cx| declare(cx, false)).unwrap();
    let column = engine.node(engine.root()).unwrap().children[0];
    assert_eq!(
        engine.node(column).unwrap().style.inset_shadow,
        Some(shadow())
    );
    let button = engine
        .node(column)
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|&id| {
            matches!(
                engine.node(id).unwrap().kind,
                Kind::Button | Kind::ButtonBase
            )
        })
        .unwrap();
    assert_eq!(
        engine.node(button).unwrap().style.inset_shadow,
        Some(shadow())
    );
    let stats = engine.frame_declarations(|cx| declare(cx, true)).unwrap();
    assert_eq!((stats.measure_calls, stats.layout_runs), (0, 0));
    assert!(engine.node(column).unwrap().style.inset_shadow.is_none());
    assert!(engine.node(button).unwrap().style.inset_shadow.is_none());
}
