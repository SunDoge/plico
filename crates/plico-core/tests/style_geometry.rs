use plico_core::{Engine, Style, prelude::*};

#[test]
fn axis_padding_cascades_and_legacy_style_defaults_survive() {
    let mut e = Engine::default();
    e.frame(|cx| {
        text(cx, "axis")
            .key("axis")
            .padding(2)
            .padding_x(14)
            .padding_y(6)
            .border(1., Color::Border);
        text(cx, "reset")
            .key("reset")
            .padding_x(20)
            .padding_y(10)
            .padding(3);
    })
    .unwrap();
    let axis = e.node(e.find(e.root(), "axis").unwrap()).unwrap();
    assert_eq!(
        (
            axis.style.horizontal_padding(),
            axis.style.vertical_padding()
        ),
        (14., 6.)
    );
    assert!((axis.size.height - (axis.style.font_size as f32 * 1.4 + 14.)).abs() < 0.01);
    let reset = e.node(e.find(e.root(), "reset").unwrap()).unwrap().style;
    assert_eq!(
        (reset.horizontal_padding(), reset.vertical_padding()),
        (3., 3.)
    );
    let legacy: Style = serde_json::from_str(r#"{"padding":7}"#).unwrap();
    assert_eq!(
        (legacy.horizontal_padding(), legacy.vertical_padding()),
        (7., 7.)
    );
    assert_eq!(legacy.border_width, 0.);
}

#[test]
fn border_color_is_paint_only_but_width_and_axis_padding_remeasure() {
    let mut e = Engine::default();
    let draw = |cx: &mut UiContext, width, color, px| {
        text(cx, "Hello")
            .key("label")
            .padding_x(px)
            .padding_y(4)
            .border(width, color);
    };
    e.frame(|cx| draw(cx, 1., Color::Border, 8)).unwrap();
    let stats = e.frame(|cx| draw(cx, 1., Color::Accent, 8)).unwrap();
    assert_eq!((stats.layout_runs, stats.measure_calls), (0, 0));
    assert_eq!(
        e.frame(|cx| draw(cx, 2., Color::Accent, 8))
            .unwrap()
            .measure_calls,
        1
    );
    assert_eq!(
        e.frame(|cx| draw(cx, 2., Color::Accent, 12))
            .unwrap()
            .measure_calls,
        1
    );
}

#[test]
fn invalid_new_geometry_is_rejected_and_state_colors_invalidate_palette() {
    for style in [
        Style {
            padding_x: Some(f32::NAN),
            ..Default::default()
        },
        Style {
            padding_y: Some(-1.),
            ..Default::default()
        },
        Style {
            border_width: f32::INFINITY,
            ..Default::default()
        },
    ] {
        assert!(!style.valid());
    }
    let a = Theme::light();
    let mut b = a;
    b.metrics = Metrics::compact();
    assert!(a.same_colors(b));
    b.control_hover ^= 1;
    assert!(!a.same_colors(b));
}
