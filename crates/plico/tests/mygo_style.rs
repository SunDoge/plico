//! Reference contract: MyGO 9b265767 ui/theme.go, widgets.go, textinput.go.
use plico::{Engine, Kind, Theme, prelude::*};

#[test]
fn light_and_dark_control_faces_match_mygo_reference() {
    for (theme, text, accent, hover, pressed, danger) in [
        (
            Theme::light(),
            0x18181b,
            0x2563eb,
            0xe9e9ec,
            0xdddde1,
            0xdc2626,
        ),
        (
            Theme::dark(),
            0xf4f4f5,
            0x3b82f6,
            0x323236,
            0x3c3c41,
            0xef4444,
        ),
    ] {
        assert_eq!(theme.foreground, text);
        assert_eq!(theme.accent, accent);
        assert_eq!(theme.control_hover, hover);
        assert_eq!(theme.control_pressed, pressed);
        assert_eq!(theme.danger, danger);
        assert_eq!(theme.metrics.spacing, 4);
        assert_eq!(theme.metrics.radius, 6);
    }
}

#[test]
fn ordinary_controls_use_reference_padding_without_shadows() {
    let mut engine = Engine::default();
    let buffer = TextBuffer::new();
    engine
        .frame_declarations(|cx| {
            button(cx).text("Ordinary");
            button(cx).text("Primary").primary();
            button(cx).text("Toolbar").ghost();
            text_input(cx, "Name", &buffer);
        })
        .unwrap();
    let children = engine.node(engine.root()).unwrap().children;
    let styles: Vec<_> = children
        .iter()
        .map(|e| engine.node(*e).unwrap().style)
        .collect();
    for style in &styles {
        assert_eq!(style.shadow, None);
        assert_eq!(style.ambient_shadow, None);
        assert_eq!(style.radius, 6.);
    }
    assert_eq!(styles[0].padding_x, Some(14.));
    assert_eq!(styles[0].padding_y, Some(6.));
    assert_eq!(styles[0].border_width, 1.);
    assert_eq!(styles[1].background, Some(Color::Accent));
    assert_eq!(styles[1].border_width, 0.);
    assert_eq!(styles[2].background, None);
    assert_eq!(styles[2].border_width, 0.);
    assert_eq!(styles[3].padding_x, Some(10.));
    assert_eq!(styles[3].padding_y, Some(6.));
    assert_eq!(engine.node(children[3]).unwrap().kind, Kind::TextInput);
}

#[test]
fn toolbar_composes_ordinary_buttons_and_restores_the_outer_appearance() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            toolbar(cx).children(|cx| {
                column(cx).children(|cx| {
                    button(cx).text("Toolbar child");
                    button(cx).text("Toolbar primary").primary();
                });
            });
            button(cx).text("Outside");
        })
        .unwrap();
    fn find(engine: &Engine, id: plico::Entity, label: &str) -> Option<plico::Entity> {
        let node = engine.node(id).unwrap();
        if matches!(node.kind, Kind::Button | Kind::ButtonBase)
            && engine.accessible_name(id).as_deref() == Some(label)
        {
            return Some(id);
        }
        node.children
            .iter()
            .find_map(|child| find(engine, *child, label))
    }
    let style = |label| {
        let id = find(&engine, engine.root(), label).unwrap();
        engine.node(id).unwrap().style
    };
    let toolbar = style("Toolbar child");
    assert_eq!(toolbar.background, None);
    assert_eq!(toolbar.border_width, 0.);
    assert_eq!(toolbar.padding_x, Some(10.));
    let primary = style("Toolbar primary");
    assert_eq!(primary.background, Some(Color::Accent));
    assert_eq!(primary.hover_background, Some(Color::AccentHover));
    assert_eq!(style("Outside").background, Some(Color::Control));
    assert_eq!(style("Outside").border_width, 1.);
}

#[test]
fn bare_editor_does_not_repeat_its_composite_field_chrome() {
    let mut engine = Engine::default();
    let buffer = TextBuffer::new();
    engine
        .frame_declarations(|cx| {
            text_input(cx, "Bare", &buffer).unstyled();
        })
        .unwrap();
    let id = engine.node(engine.root()).unwrap().children[0];
    let style = engine.node(id).unwrap().style;
    assert_eq!(style.padding, 0.);
    assert_eq!(style.padding_x, None);
    assert_eq!(style.padding_y, None);
    assert_eq!(style.min_height, Some(20.));
    assert_eq!(style.background, None);
    assert_eq!(style.border_width, 0.);
    assert!(!style.control_chrome);
}

#[test]
fn calendar_uses_mygo_day_geometry_and_accent_selection_without_an_extra_card() {
    use plico::{CalendarDate, CalendarState, WeekStart};
    let mut engine = Engine::default();
    let selected = CalendarDate::new(2026, 10, 8).unwrap();
    let today = CalendarDate::new(2026, 10, 4).unwrap();
    let mut state = CalendarState::new(selected).with_selected(selected);
    engine
        .frame_declarations(|cx| {
            calendar_with(cx, &mut state, WeekStart::Sunday, Some(today));
        })
        .unwrap();
    fn find(engine: &Engine, id: plico::Entity, chosen: bool, today: bool) -> Option<plico::Style> {
        let node = engine.node(id).unwrap();
        if node.kind == Kind::ButtonBase
            && ((chosen && node.style.background == Some(Color::Accent))
                || (today && node.style.border_color == Color::Accent))
        {
            return Some(node.style);
        }
        node.children
            .iter()
            .find_map(|id| find(engine, *id, chosen, today))
    }
    let day = find(&engine, engine.root(), true, false).unwrap();
    assert_eq!(day.height, Some(28.));
    assert_eq!(day.radius, 6.);
    assert_eq!(day.foreground, Color::OnAccent);
    assert_eq!(day.border_width, 0.);
    assert_eq!(
        find(&engine, engine.root(), false, true)
            .unwrap()
            .border_width,
        1.
    );
    let outer = engine
        .node(engine.node(engine.root()).unwrap().children[0])
        .unwrap();
    assert_eq!(outer.style.background, None);
    assert_eq!(outer.style.border_width, 0.);
}

#[test]
fn badges_avatars_and_progress_use_the_mygo_reference_recipes() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            badge(cx, "12");
            avatar(cx).fallback("A");
            progress(cx, "Download", 0.5);
        })
        .unwrap();
    let children = engine.node(engine.root()).unwrap().children;
    let badge = engine.node(children[0]).unwrap().style;
    assert_eq!(badge.padding_x, Some(6.));
    assert_eq!(badge.padding_y, Some(0.));
    assert_eq!(badge.background, Some(Color::Foreground));
    assert_eq!(badge.background_alpha, 0.1);
    assert_eq!(badge.corner_smoothing, 1.);
    let avatar = engine.node(children[1]).unwrap();
    assert_eq!(avatar.style.width, Some(31.5));
    assert_eq!(avatar.style.height, Some(31.5));
    assert_eq!(avatar.style.corner_smoothing, 1.);
    assert_eq!(avatar.style.background, Some(Color::Literal(0xbf6d58)));
    assert_eq!(
        engine.node(avatar.children[0]).unwrap().style.font_weight,
        600
    );
    let progress = engine.node(children[2]).unwrap();
    let track = engine.node(progress.children[1]).unwrap().style;
    assert_eq!(track.height, Some(6.));
    assert_eq!(track.radius, 3.);
    assert_eq!(track.background, Some(Color::Border));
}
