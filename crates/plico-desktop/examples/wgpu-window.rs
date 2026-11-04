//! The normal Plico shell and immediate API, with the experimental wgpu backend.
//! Runtime text, shape, clip and input dispatch are shared with other backends.
use plico::prelude::*;
#[derive(Default)]
struct Counter {
    count: i32,
    reported: Option<i32>,
}
impl plico::View for Counter {
    fn theme(&self) -> Theme {
        Theme::light()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        content(cx, &mut self.count);
        if self.reported != Some(self.count) {
            println!("count={}", self.count);
            self.reported = Some(self.count);
        }
    }
}
#[component]
#[rustfmt::skip]
fn content(cx: &mut UiContext<'_>, count: &mut i32) {
    column(cx).width_percent(100.).height_percent(100.).padding(32).gap(18).background(Color::Surface) + |cx| {
        text(cx, "Plico / wgpu").font_size(26).weight(600);
        text(cx, "Shared window host, layout and input dispatch").font_size(14).color(Color::Muted);
        column(cx).padding(24).gap(16).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
            text(cx, "A normal Plico component").font_size(18).weight(600);
            text!(cx, "Count: {}", count).font_size(32);
            text(cx, "中文与日本語の文字 · café · 👩‍💻").font_size(14);
            row(cx).gap(10) + |cx| {
                if button(cx).text("Increment").primary().clicked() {
                    *count += 1;
                }
                if button(cx).text("Reset").ghost().clicked() {
                    *count = 0;
                }
            };
        };
    };
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run_view_with_options(
        Counter::default(),
        plico_desktop::WindowOptions::new()
            .title("Plico — wgpu shared host")
            .renderer(plico_desktop::RenderBackend::Wgpu)
            .initial_inner_size(620, 410)
            .min_inner_size(520, 370),
    )
}
