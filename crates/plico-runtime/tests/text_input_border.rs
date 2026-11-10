//! Text inputs keep their fill and move hover/focus feedback to the border,
//! following MyGO's `inputBorder`.
use plico::Theme;
use plico::prelude::*;
use plico_runtime::{
    HostView,
    testing::{Command, Driver},
};

#[derive(Default)]
struct Demo {
    name: String,
}

impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        text_input_string(cx, "Name", &mut self.name);
    }
}

fn channels(packed: u32) -> [u8; 3] {
    [
        ((packed >> 16) & 255) as u8,
        ((packed >> 8) & 255) as u8,
        (packed & 255) as u8,
    ]
}

fn input_border(ui: &Driver<Demo>) -> [u8; 3] {
    let (_, color) = ui
        .source
        .adapter
        .placed
        .iter()
        .find_map(|node| node.paint.border)
        .expect("the text input paints a border");
    let rgba = color.to_rgba8();
    [rgba.r, rgba.g, rgba.b]
}

#[test]
fn text_input_border_tracks_hover_and_focus_without_changing_its_fill() {
    let theme = Theme::default();
    let mut ui = Driver::new(Demo::default(), 480, 240).unwrap();
    assert_eq!(input_border(&ui), channels(theme.border));
    let fill = ui
        .source
        .adapter
        .placed
        .iter()
        .find(|node| node.paint.border.is_some())
        .and_then(|node| node.paint.background)
        .expect("the text input keeps a fill");
    assert_eq!(channels(theme.control), {
        let rgba = fill.to_rgba8();
        [rgba.r, rgba.g, rgba.b]
    });

    let (x, y) = ui
        .source
        .adapter
        .placed
        .iter()
        .find(|node| node.paint.border.is_some())
        .map(|node| {
            (
                (node.rect[0] + node.rect[2]) / 2.,
                (node.rect[1] + node.rect[3]) / 2.,
            )
        })
        .unwrap();
    ui.execute(Command::PointerMove {
        x: x as f64,
        y: y as f64,
    })
    .unwrap();
    let hovered = theme.blend(Color::Border, Color::Foreground, 0.25);
    assert_eq!(
        input_border(&ui),
        channels(theme.resolve(hovered)),
        "hover mixes the default border 25% toward text"
    );

    ui.execute(Command::Focus {
        role: "textbox".into(),
        label: "Name".into(),
        occurrence: None,
    })
    .unwrap();
    assert_eq!(
        input_border(&ui),
        channels(theme.accent),
        "focus turns the border accent"
    );
}
