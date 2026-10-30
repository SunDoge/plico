use plico::prelude::*;
use plico_renderer::{FrameSource, TextContext};
use plico_runtime::{HostView, Source};
struct Demo {
    family: FontFamily,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        text(cx, "let value = 42;").font_family(self.family);
        text(cx, "Window font");
        column(cx)
            .font_family(FontFamily::Monospace)
            .children(|cx| {
                button(cx).text("Inherited code");
            });
    }
}
#[test]
fn generic_families_reach_text_layout_and_clean_frames_keep_the_paint_cache() {
    let mut source = Source::new(Demo {
        family: FontFamily::Monospace,
    })
    .with_font_family("Noto Sans");
    let mut text = TextContext::new();
    let nodes = source.build_frame(&mut text, 500, 300);
    let code = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("let value = 42;"))
        .unwrap();
    assert_eq!(code.paint.font_family.as_deref(), Some("monospace"));
    let inherited = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Inherited code"))
        .unwrap();
    assert_eq!(inherited.paint.font_family.as_deref(), Some("monospace"));
    let normal = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Window font"))
        .unwrap();
    assert_eq!(normal.paint.font_family.as_deref(), Some("Noto Sans"));
    source.build_frame(&mut text, 500, 300);
    assert_eq!(source.adapter.stats.paint_writes, 0);
    source.view.family = FontFamily::Serif;
    source.request_rebuild();
    let nodes = source.build_frame(&mut text, 500, 300);
    let code = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("let value = 42;"))
        .unwrap();
    assert_eq!(code.paint.font_family.as_deref(), Some("serif"));
    assert!(source.adapter.stats.layout_runs > 0);
}
