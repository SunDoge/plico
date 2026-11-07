//! Same small layout written in Rust, for edit/build feedback comparison.
use plico::prelude::*;
use plico_runtime::{HostView, Source};
#[derive(Default)]
struct Demo {
    count: i32,
    details: bool,
    text: String,
    font: f64,
}
impl HostView for Demo {
    fn theme(&self) -> Theme {
        Theme {
            surface: 0xffffff,
            accent: 0xaf3652,
            accent_hover: 0x982c46,
            accent_pressed: 0x81243b,
            selection: 0xf9edf0,
            ..Theme::light()
        }
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(32).gap(20).background(Color::Panel) + |cx| {
            text(cx, "Plico, from Rust").font_size(28).weight(700);
            text(cx, "One typed batch. Native text, layout and rendering.").color(Color::Muted);
            row(cx).width_percent(100.).gap(24).align(Align::Start) + |cx| {
                column(cx).width(380.).gap(16) + |cx| {
                    text(cx, "Counter").font_size(20).weight(600);
                    text!(cx, "Count: {}", self.count).font_size(36).weight(650);
                    row(cx).gap(8) + |cx| {
                        if button(cx).text("Increment").primary().clicked() {
                            self.count += 1;
                        }
                        if button(cx).text("Decrement").ghost().clicked() {
                            self.count -= 1;
                        }
                        if button(cx).text("Reset").clicked() {
                            self.count = 0;
                        }
                    };
                    checkbox(cx, "Show details", &mut self.details);
                    if self.details {
                        column(cx).padding(16).gap(8).background(Color::Control).radius(8.) + |cx| {
                            text(cx, "Ordinary fields, ordinary if/for.").weight(600);
                            text(cx, "No signal wrapper or borrowed callback state.").color(Color::Muted);
                        };
                    }
                    text(cx, "Translation").weight(600);
                    text_input_string(cx, "Translation", &mut self.text).width_percent(100.);
                    text(cx, "Font size").weight(600);
                    slider(cx, "Font size", &mut self.font, 12.0..=48.0);
                };
                column(cx).width(0.).grow(1.).padding(24).gap(20).background(Color::Literal(0xf9f9fa))
                    .radius(12.) + |cx|
                {
                    text(cx, "Live preview").font_size(18).weight(600);
                    text(cx, &self.text).font_size(self.font as u16).width_percent(100.);
                    row(cx).gap(8) + |cx| {
                        spinner(cx);
                        text(cx, "Native animation; no C# frame rebuild.").color(Color::Muted);
                    };
                };
            };
        };
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(usage_rs::Cli)]
    struct Options {
        #[usage(long)]
        capture: Option<std::path::PathBuf>,
    }
    let options = Options::parse();
    let source = Source::new(Demo {
        details: true,
        font: 28.,
        text: "在 C# 中构建界面，使用 Plico 的系统文字与 wgpu 渲染。".into(),
        ..Default::default()
    });
    if let Some(path) = options.capture {
        let mut source = source;
        plico_desktop::capture(&mut source)
            .size(960, 720)
            .write_png(path)?;
    } else {
        plico_desktop::run_source_with_options(
            source,
            plico_desktop::WindowOptions::new()
                .title("Plico Rust comparison")
                .initial_inner_size(1040, 780),
        )?;
    }
    Ok(())
}
