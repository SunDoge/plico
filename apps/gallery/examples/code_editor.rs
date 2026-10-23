//! Native editor gallery: cargo run -p plico-gallery --example code_editor
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), plico_vello::DesktopError> {
    use plico_core::prelude::*;
    use plico_editor::{CodeEditor, CodeEditorState};
    use plico_vello::{SurfaceSource, View, WindowOptions};

    struct Demo {
        state: CodeEditorState,
    }
    impl View for Demo {
        fn theme(&self) -> Theme {
            Theme::dark()
        }
        #[component]
        #[rustfmt::skip]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).width_percent(100.).height_percent(100.).padding(20).gap(12).background(Color::Literal(0x10151e))
                .children(|cx| {
                row(cx).gap(12).align(Align::Center).children(|cx| {
                    svg(cx, &plico_gallery::icons::CODE).size(22.,22.).color(Color::Literal(0x89b4fa));
                    text(cx, "Code editor").font_size(24).weight(700).color(Color::Literal(0xe2e8f0)).grow(1.);
                    if button(cx).text("Undo").clicked() { self.state.undo(); }
                    if button(cx).text("Redo").clicked() { self.state.redo(); }
                });
                text(cx,"Floem editing core · Parley text · Rust highlighting").color(Color::Literal(0x94a3b8));
                column(cx).key("code-editor").grow(1.).width_percent(100.).clip(true).radius(10.)
                    .background(Color::Literal(0x181d27))
                + |_cx|
                {};
                text(cx,"Click to edit · Ctrl/Cmd+Z undo · Shift+arrows select · Ctrl/Cmd+C/V clipboard").font_size(12)
                    .color(Color::Literal(0x94a3b8));
            })
        }
    }
    let state = CodeEditorState::new(
        "// A small native editor, backed by Floem.\n// 中文输入、选区、撤销：试着编辑这里。\n\nfn main() {\n    let greeting = \"Hello, Plico!\";\n    println!(\"{greeting}\");\n}\n",
    );
    let source = SurfaceSource::new(
        Demo {
            state: state.clone(),
        },
        CodeEditor::new(state),
        "code-editor",
    );
    plico_vello::run_source_with_options(
        source,
        WindowOptions::new()
            .title("Plico · Code editor")
            .initial_inner_size(1000, 700),
    )
}
#[cfg(target_arch = "wasm32")]
fn main() {}
