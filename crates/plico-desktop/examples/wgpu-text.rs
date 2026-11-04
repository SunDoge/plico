//! System-text window acceptance fixture, without shader or application-font requirements.
//! Run with --no-default-features --features wgpu --example wgpu-text.
use plico::prelude::*;

#[derive(Default)]
struct TextFixture {
    input: String,
    large: bool,
    revision: u32,
}
impl plico::View for TextFixture {
    fn theme(&self) -> Theme {
        Theme::light()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        fixture(cx, self);
    }
}
#[component]
#[rustfmt::skip]
fn fixture(cx: &mut UiContext<'_>, state: &mut TextFixture) {
    column(cx).width_percent(100.).height_percent(100.).padding(20).gap(12).background(Color::Surface) + |cx| {
        text(cx, "System text / wgpu").font_size(26).weight(600);
        text(cx, "Scroll / resize / edit / move between displays.").font_size(14).color(Color::Muted);
        row(cx).gap(12).align(Align::Center) + |cx| {
            if button(cx).text(if state.large { "Normal type" } else { "Large type" }).clicked() {
                state.large = !state.large;
            }
            if button(cx).text("Update one row").primary().clicked() {
                state.revision += 1;
            }
        };
        text_input_string(cx, "Native editing", &mut state.input).width_percent(100.)
            .placeholder("Type / paste / select: Latin, 中文, 日本語, العربية");
        text(cx, "System fonts: Latin / Chinese / Japanese / Arabic / Hebrew.").font_size(13).color(Color::Muted);
        column(cx).width_percent(100.).height(0.).grow(1.).scroll_y(true).gap(8).padding(12)
            .background(Color::Panel).radius(8.) + |cx|
        {
            for index in 0..120u64 {
                column(cx).key(index).width_percent(100.).gap(3).padding(8).border(1., Color::Border).radius(4.)
                    .shrink(0.) + |cx|
                {
                    let size = if state.large { 22 } else { 14 };
                    text(
                        cx,
                        match index % 4 {
                            0 => "Hamburgefontsiv · ffi fl · 0123456789 · café",
                            1 => "中文笔画与标点：横竖撇捺，测试文字。",
                            2 => "日本語の表示・ひらがな・カタカナ・漢字",
                            _ => "مرحبا بالعالم — שלום עולם",
                        },
                    ).font_size(size).weight(if index % 3 == 0 { 500 } else { 400 });
                    text!(
                        cx,
                        "Row {} / revision {}",
                        index,
                        if index == 2 { state.revision } else { 0 }
                    ).font_size(12).color(Color::Muted);
                };
            }
        };
    };
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    plico_desktop::run_view_with_options(
        TextFixture::default(),
        plico_desktop::WindowOptions::new()
            .title("Plico — native text acceptance")
            .renderer(plico_desktop::RenderBackend::Wgpu)
            .initial_inner_size(780, 680)
            .min_inner_size(540, 400),
    )
}
