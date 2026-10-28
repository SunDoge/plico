use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, TextContext};
#[derive(Default)]
struct Demo {
    large: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).children(|cx| {
            text(cx, "Mixed 中文 🇯🇵").font_size(if self.large { 22 } else { 14 });
            button(cx).text("Action");
        });
    }
}
fn assert_family(source: &Source<Demo>, family: &str) {
    let labels: Vec<_> = source
        .adapter
        .placed
        .iter()
        .filter(|node| node.paint.text.is_some())
        .collect();
    assert!(labels.len() >= 2);
    assert!(
        labels
            .iter()
            .all(|node| node.paint.font_family.as_deref() == Some(family))
    );
}
#[test]
fn configured_fonts_survive_style_updates_and_switching_invalidates_measurement() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 500, 400);
    assert_family(&source, plico_renderer::text::DEFAULT_UI_FONT_FAMILY);
    source.set_font_family("serif");
    source.build_frame(&mut text, 500, 400);
    assert!(source.adapter.stats.layout_runs > 0);
    assert_family(&source, "serif");
    source.set_font_family("monospace");
    source.build_frame(&mut text, 500, 400);
    assert!(source.adapter.stats.layout_runs > 0);
    assert_family(&source, "monospace");
    source.view.large = true;
    source.request_rebuild();
    source.build_frame(&mut text, 500, 400);
    assert_family(&source, "monospace");
    source.set_font_family("monospace");
    source.build_frame(&mut text, 500, 400);
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert!(source.last_error.is_none());
}
