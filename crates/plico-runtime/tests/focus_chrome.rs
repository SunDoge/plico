use plico::prelude::*;
use plico_runtime::{
    HostView,
    testing::{Command, Driver},
};
#[derive(Default)]
struct Demo {
    first: String,
    second: String,
    ordinary: String,
}
#[component]
fn field(cx: &mut UiContext, label: &str, value: &mut String) {
    row(cx)
        .radius(20.)
        .border(1., Color::Literal(0xffffff))
        .focus_within_border(1., Color::Literal(0x7294a6))
        .children(|cx| {
            text_input_string(cx, label, value).unstyled();
        });
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).children(|cx| {
            field(cx, "First", &mut self.first);
            field(cx, "Second", &mut self.second);
            text_input_string(cx, "Ordinary", &mut self.ordinary);
        });
    }
}
fn focused_borders(ui: &Driver<Demo>) -> usize {
    fn count(nodes: &[plico::ViewNode]) -> usize {
        nodes
            .iter()
            .map(|node| {
                usize::from(node.style.border_color == Color::Literal(0x7294a6))
                    + count(&node.children)
            })
            .sum()
    }
    count(&ui.declarations())
}
#[test]
fn composite_focus_follows_scoped_descendants_without_hiding_ordinary_input_focus() {
    let mut ui = Driver::new(Demo::default(), 600, 400).unwrap();
    assert_eq!(focused_borders(&ui), 0);
    for label in ["First", "Second"] {
        ui.execute(Command::Focus {
            role: "textbox".into(),
            label: label.into(),
            occurrence: None,
        })
        .unwrap();
        assert_eq!(focused_borders(&ui), 1);
        assert!(
            ui.source
                .adapter
                .placed
                .iter()
                .all(|n| n.paint.outline_width == 0.)
        );
    }
    ui.execute(Command::Focus {
        role: "textbox".into(),
        label: "Ordinary".into(),
        occurrence: None,
    })
    .unwrap();
    assert_eq!(focused_borders(&ui), 0);
    assert!(
        ui.source
            .adapter
            .placed
            .iter()
            .any(|n| n.paint.outline_width > 0.)
    );
}

#[derive(Default)]
struct SearchDemo {
    query: String,
}
impl HostView for SearchDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        search_field(cx, "Search mail", "Search", &mut self.query);
    }
}
#[test]
fn builtin_search_uses_one_outer_focus_border_and_keeps_native_text_input() {
    let mut ui = Driver::new(SearchDemo::default(), 600, 400).unwrap();
    ui.execute(Command::Focus {
        role: "textbox".into(),
        label: "Search mail".into(),
        occurrence: None,
    })
    .unwrap();
    ui.execute(Command::Text {
        text: "hello".into(),
    })
    .unwrap();
    assert_eq!(ui.source.view.query, "hello");
    assert!(
        ui.source
            .adapter
            .placed
            .iter()
            .all(|node| node.paint.outline_width == 0.)
    );
    fn focused(nodes: &[plico::ViewNode]) -> usize {
        nodes
            .iter()
            .map(|node| {
                usize::from(
                    node.kind == plico::Kind::Row && node.style.border_color == Color::Focus,
                ) + focused(&node.children)
            })
            .sum()
    }
    assert_eq!(focused(&ui.declarations()), 1);
}
