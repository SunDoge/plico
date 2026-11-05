//! Toolbar composition must produce real flexible layout, not an unused builder.
use plico::{
    UiContext,
    widgets::{button, toolbar, toolbar_spacer},
};
use plico_renderer::{FrameSource, TextContext};
use plico_runtime::{HostView, Source};

struct Toolbar;
impl HostView for Toolbar {
    fn show(&mut self, cx: &mut UiContext) {
        toolbar(cx).children(|cx| {
            button(cx).text("Leading");
            toolbar_spacer(cx);
            button(cx).text("Trailing");
        });
    }
}

#[test]
fn spacer_absorbs_extra_width_and_keeps_trailing_action_at_the_end() {
    let mut source = Source::new(Toolbar);
    let mut text = TextContext::new();
    let mut edges = Vec::new();
    for width in [400, 700] {
        source.build_frame(&mut text, width, 200);
        let find = |label: &str| {
            source
                .adapter
                .placed
                .iter()
                .find(|placed| placed.paint.text.as_deref() == Some(label))
                .unwrap()
                .rect
        };
        let leading = find("Leading");
        let trailing = find("Trailing");
        assert!(trailing[0] > leading[2] + 100.);
        edges.push((leading[0], width as f32 - trailing[2]));
    }
    assert!((edges[0].0 - edges[1].0).abs() < 1.);
    assert!((edges[0].1 - edges[1].1).abs() < 1.);
}
