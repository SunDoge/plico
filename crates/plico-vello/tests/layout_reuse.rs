use plico_core::{Theme, UiContext};
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
    fn show(&mut self, cx: &mut UiContext<'_>) {
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

#[test]
fn rich_text_color_animation_repaints_without_remeasurement() {
    use plico_core::{Color, TextSpan};
    use plico_renderer::Scene;
    struct RichText {
        color: Color,
        weight: u16,
    }
    impl View for RichText {
        fn show(&mut self, cx: &mut UiContext<'_>) {
            cx.text("Animated highlight").spans(&[TextSpan {
                range: 0..8,
                weight: self.weight,
                italic: false,
                color: Some(self.color),
            }]);
        }
    }
    let mut source = Source::new(RichText {
        color: Color::Literal(0xff0000),
        weight: 400,
    });
    let mut text = TextContext::new();
    let nodes = source.build_frame(&mut text, 400, 300);
    let rects: Vec<_> = nodes.iter().map(|n| n.rect).collect();
    let mut before = Scene::new();
    source.paint_scene(&mut before, &nodes, &mut text, (400, 300), 1.);
    source.view.color = Color::Literal(0x0000ff);
    source.request_rebuild();
    let nodes = source.build_frame(&mut text, 400, 300);
    assert_eq!(source.adapter.stats.text_measure_invalidations, 0);
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert_eq!(nodes.iter().map(|n| n.rect).collect::<Vec<_>>(), rects);
    let mut after = Scene::new();
    source.paint_scene(&mut after, &nodes, &mut text, (400, 300), 1.);
    assert_ne!(before, after, "color must still reach drawing commands");
    source.view.weight = 700;
    source.request_rebuild();
    source.build_frame(&mut text, 400, 300);
    assert!(source.adapter.stats.text_measure_invalidations > 0);
    assert_eq!(source.adapter.stats.layout_runs, 1);
}
