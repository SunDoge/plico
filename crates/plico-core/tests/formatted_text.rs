use plico_core::widgets::{column, text};
use plico_core::{Engine, UiContext, component, text};

fn texts(engine: &Engine) -> Vec<String> {
    fn walk(engine: &Engine, id: plico_core::Entity, out: &mut Vec<String>) {
        let node = engine.node(id).unwrap();
        if !node.text.is_empty() {
            out.push(node.text.to_owned());
        }
        for id in node.children {
            walk(engine, *id, out);
        }
    }
    let mut out = Vec::new();
    walk(engine, engine.root(), &mut out);
    out
}

#[component]
fn screen(cx: &mut UiContext, extra: bool, n: u32) {
    column(cx).gap(8).children(|cx| {
        if extra {
            text!(cx, "Optional {n}");
        }
        text!(cx, "Value: {n:04}").font_size(20).weight(600);
        text(cx, "plain");
    })
}

#[test]
fn formatted_text_keeps_identity_across_branches_and_reuses_unchanged_content() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| screen(cx, false, 12))
        .unwrap();
    let parent = engine.node(engine.root()).unwrap().children[0];
    let value = engine.node(parent).unwrap().children[0];
    assert_eq!(engine.node(value).unwrap().text, "Value: 0012");
    engine
        .frame_declarations(|cx| screen(cx, true, 13))
        .unwrap();
    assert_eq!(engine.node(parent).unwrap().children[1], value);
    assert_eq!(engine.node(value).unwrap().text, "Value: 0013");
    let stats = engine
        .frame_declarations(|cx| screen(cx, true, 13))
        .unwrap();
    assert_eq!(stats.text_writes, 0);
    assert_eq!(engine.node(value).unwrap().style.font_size, 20);
}

#[test]
fn format_arguments_are_consumed_synchronously_and_use_standard_formatting() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            let name = String::from("Plico");
            text!(
                cx,
                "{name}: {value:.precision$}",
                value = 1.2345,
                precision = 2
            );
            drop(name);
            plico_core::text!(cx, "{:?}", [1, 2]);
            cx.text_fmt(format_args!("literal"));
            plico_core::widgets::text_fmt(cx, format_args!("{}", ""));
        })
        .unwrap();
    assert_eq!(texts(&engine), ["Plico: 1.23", "[1, 2]", "literal"]);
}

#[test]
fn format_macro_works_inside_standalone_view_and_formats_each_argument_once() {
    struct Count<'a>(&'a std::cell::Cell<u32>);
    impl std::fmt::Display for Count<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.set(self.0.get() + 1);
            f.write_str("counted")
        }
    }
    let calls = std::cell::Cell::new(0);
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            let _ = column(cx).children(|cx| {
                text!(cx, "{}", Count(&calls));
            });
        })
        .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(texts(&engine), ["counted"]);
}
