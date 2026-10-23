use plico_core::{Theme, Ui};
use plico_renderer::{FrameSource, TextContext};
use plico_vello::{Source, View};

struct Panel {
    theme: Theme,
    width: f32,
}
impl View for Panel {
    fn theme(&self) -> Theme {
        self.theme
    }
    fn show(&mut self, cx: &mut Ui<'_>) {
        cx.column()
            .width(self.width)
            .height(40.)
            .background(plico_core::Color::Panel)
            .children(|cx| {
                cx.text("Stable");
            });
    }
}
#[test]
fn color_theme_reuses_layout_but_size_and_viewport_still_update() {
    let mut source = Source::new(Panel {
        theme: Theme::default(),
        width: 100.,
    });
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 300);
    assert!(source.adapter.stats.layout_style_builds > 0);
    source.view.theme.panel ^= 0x222222;
    source.request_rebuild();
    source.build_frame(&mut text, 400, 300);
    assert!(source.adapter.stats.sync_visits > 0);
    assert_eq!(source.adapter.stats.layout_style_builds, 0);
    assert_eq!(source.adapter.stats.style_writes, 0);
    assert_eq!(source.adapter.stats.layout_runs, 0);
    source.view.width = 180.;
    source.request_rebuild();
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.adapter.stats.layout_style_builds, 1);
    assert_eq!(source.adapter.stats.style_writes, 1);
    source.build_frame(&mut text, 600, 500);
    assert!(source.adapter.stats.layout_style_builds >= 1);
    assert!(source.adapter.stats.style_writes >= 1);
    assert_eq!(source.adapter.stats.layout_runs, 1);
}
