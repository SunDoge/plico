//! The normal Plico shell and immediate API, with the experimental wgpu backend.
//! Runtime text, shape, clip and input dispatch are shared with other backends.
use plico::prelude::*;
static PROGRESS: plico::ShaderAsset = plico::shader_asset!("assets/progress.wgsl");
#[derive(Default)]
struct Counter {
    count: i32,
    reported: Option<i32>,
    input: String,
    reported_input: Option<String>,
    animated: bool,
}
impl plico::View for Counter {
    fn theme(&self) -> Theme {
        Theme::light()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        content(cx, &mut self.count, &mut self.input, &mut self.animated);
        if self.reported_input.as_ref() != Some(&self.input) {
            println!("input={:?}", self.input);
            self.reported_input = Some(self.input.clone());
        }
        if self.reported != Some(self.count) {
            println!("count={}", self.count);
            self.reported = Some(self.count);
        }
    }
}
#[component]
#[rustfmt::skip]
fn content(cx: &mut UiContext<'_>, count: &mut i32, value: &mut String, animated: &mut bool) {
    column(cx).width_percent(100.).height_percent(100.).padding(32).gap(18).background(Color::Surface) + |cx| {
        text(cx, "Plico / wgpu").font_size(26).weight(600);
        text(cx, "Shared window host, layout and input dispatch").font_size(14).color(Color::Muted);
        column(cx).padding(24).gap(16).radius(12.).background(Color::Panel).border(1., Color::Border).opacity(0.94)
        + |cx|
        {
            text(cx, "A normal Plico component").font_size(18).weight(600);
            text!(cx, "Count: {}", count).font_size(32);
            text(cx, "中文与日本語の文字 · café · 👩‍💻").font_size(14);
            text_input_string(cx, "Native input", value).placeholder("Type here · 中文 · 日本語");
            row(cx).gap(10) + |cx| {
                if button(cx).text("Increment").primary().clicked() {
                    *count += 1;
                }
                if button(cx).text("Reset").ghost().clicked() {
                    *count = 0;
                }
                if button(cx).text(if *animated { "Pause effect" } else { "Animate effect" }).ghost()
                    .clicked()
                {
                    *animated = !*animated;
                }
            };
            shader(cx, &PROGRESS).width(380.).height(12.).radius(6.)
                .values(&[count.rem_euclid(11) as f32 / 10.]).animated(*animated);
        };
    };
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run_view_with_options(
        Counter::default(),
        plico_desktop::WindowOptions::new()
            .title("Plico — wgpu shared host")
            .renderer(plico_desktop::RenderBackend::Wgpu)
            .initial_inner_size(620, 480)
            .min_inner_size(520, 450),
    )
}
