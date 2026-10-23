use plico_core::{Engine, prelude::*};
use plico_renderer::{FrameSource, PaintCommand, TextContext, layout::PlacedNode};
use plico_vello::{Source, View};
use std::time::Duration;
use vello_common::kurbo::Shape;
static ICON: SvgAsset = SvgAsset::new(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2Z" fill="#6597ff"/></svg>"##,
);
const LONG: &str = "A longer notification label that must wrap inside its content box";
#[derive(Default)]
struct Demo {
    compact: bool,
    font: u16,
    checked: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).gap(8).children(|cx| {
            switch(cx, "Notifications", &mut self.checked).font_size(self.font);
            checkbox(cx, "Save automatically", &mut self.checked).font_size(self.font);
            switch(cx, LONG, &mut self.checked)
                .font_size(self.font)
                .width(200.);
            button(cx).text("Icon action").primary().children(|cx| {
                svg(cx, &ICON).size(18., 18.);
                text(cx, "Action label").font_size(self.font);
            });
        })
    }
    fn theme(&self) -> Theme {
        let mut theme = Theme::light();
        if self.compact {
            theme.metrics = Metrics::compact();
        }
        theme
    }
}
fn marker_center(node: &PlacedNode) -> f64 {
    match &node.paint.widget.as_ref().unwrap().commands[0] {
        PaintCommand::Fill { shape, .. } => shape.bounding_box().center().y,
        _ => panic!("expected indicator"),
    }
}
fn check(s: &Source<Demo>, tcx: &mut TextContext) {
    for node in &s.adapter.placed {
        if node.paint.widget.is_none() {
            continue;
        }
        let layout = plico_renderer::scene::layout_node_text(tcx, node).unwrap();
        let line_height = layout.lines().next().unwrap().metrics().line_height;
        assert!(
            (marker_center(node) - line_height as f64 / 2.).abs() < 0.001,
            "{:?}: marker {}, first line {}",
            node.paint.text,
            marker_center(node),
            line_height
        );
        // The painted label must wrap within the same width Taffy measured.
        assert!(
            layout.height() <= node.content_size[1] + 1.,
            "{:?} content {:?} rect {:?}, measured? painted height {} width {}",
            node.paint.text,
            node.content_size,
            node.rect,
            layout.height(),
            layout.width()
        );
        if node.paint.text.as_deref() == Some(LONG) {
            assert!(layout.len() > 1);
            assert!(marker_center(node) < layout.height() as f64 / 2.);
        }
    }
    let icon = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.svg.is_some())
        .unwrap();
    let label = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Action label"))
        .unwrap();
    let layout = plico_renderer::scene::layout_node_text(tcx, label).unwrap();
    let text_center = label.content_origin[1] + layout.height() / 2.;
    // Taffy rounds each box edge; the shaped line retains fractional height.
    assert!(
        ((icon.rect[1] + icon.rect[3]) / 2. - text_center).abs() <= 1.,
        "font {} icon {:?}, label {:?}, content {:?}, line height {}",
        s.view.font,
        icon.rect,
        label.rect,
        label.content_origin,
        layout.height()
    );
}
#[test]
fn indicators_and_svg_labels_align_across_density_font_sizes_and_wrapping() {
    for compact in [false, true] {
        for font in [12, 16, 24, 32] {
            let mut s = Source::new(Demo {
                compact,
                font,
                ..Default::default()
            });
            let mut tcx = TextContext::new();
            s.build_frame(&mut tcx, 480, 1000);
            check(&s, &mut tcx);
            s.build_frame(&mut tcx, 320, 1200); // also recompute after a resize
            check(&s, &mut tcx);
        }
    }
}
#[test]
fn paint_only_toggle_animation_keeps_alignment_without_layout() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(
        Demo {
            compact: true,
            font: 24,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 480, 1000);
    s.view.checked = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 480, 1000);
    clock.set(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut tcx, 480, 1000);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    check(&s, &mut tcx);
    clock.set(Duration::from_millis(140)).unwrap();
    s.build_frame(&mut tcx, 480, 1000);
    check(&s, &mut tcx);
    assert!(!s.has_anim());
}

#[test]
fn paint_and_measure_use_the_content_width_of_padded_text() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            text(cx, LONG).width(180.).padding(20);
        })
        .unwrap();
    let mut adapter = plico_vello::Adapter::default();
    let mut tcx = TextContext::new();
    let nodes = adapter.layout(&mut engine, &mut tcx, 300, 500);
    let node = nodes.iter().find(|n| n.paint.text.is_some()).unwrap();
    let layout = plico_renderer::scene::layout_node_text(&mut tcx, node).unwrap();
    assert!(
        layout.height() <= node.content_size[1] + 1.,
        "content {:?}, rect {:?} paint height {} width {}",
        node.content_size,
        node.rect,
        layout.height(),
        layout.width()
    );
    assert!(layout.len() > 1);
    assert!(layout.width() <= node.content_size[0] + 1.);
}
