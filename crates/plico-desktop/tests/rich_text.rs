use plico::{Color, TextSpan, prelude::*};
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, TextContext};
struct Paragraph {
    styled: bool,
}
impl View for Paragraph {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let spans = [TextSpan {
            range: 7..11,
            weight: 700,
            italic: true,
            color: Some(Color::Accent),
        }];
        text(cx, "中文 bold paragraph").spans(if self.styled { &spans } else { &[] });
    }
}
#[test]
fn styled_ranges_reach_shaper_and_can_be_removed_without_replacing_node() {
    let mut source = Source::new(Paragraph { styled: true });
    let mut tcx = TextContext::new();
    // Byte offsets are validated, including non-ASCII boundaries.
    source.build_frame(&mut tcx, 640, 400);
    assert!(source.last_error.is_none());
    let node = source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.is_some())
        .unwrap();
    let id = node.node_id;
    assert_eq!(node.paint.text_runs.len(), 1);
    assert_eq!(node.paint.text_runs[0].font_weight, 700.);
    assert!(node.paint.text_runs[0].font_italic);
    source.build_frame(&mut tcx, 640, 400);
    assert_eq!(source.adapter.stats.paint_writes, 0);
    source.view.styled = false;
    source.build_frame(&mut tcx, 640, 400);
    let node = source
        .adapter
        .placed
        .iter()
        .find(|p| p.node_id == id)
        .unwrap();
    assert!(node.paint.text_runs.is_empty());
    assert_eq!(source.adapter.stats.created, 0);
    assert!(source.adapter.stats.text_measure_invalidations > 0);
}
#[test]
fn protocol_rejects_invalid_utf8_span_and_roundtrips_valid_spans() {
    let mut engine = plico::Engine::default();
    engine
        .frame_declarations(|cx| {
            text(cx, "中文 bold").spans(&[TextSpan {
                range: 7..11,
                weight: 700,
                italic: false,
                color: None,
            }]);
        })
        .unwrap();
    let nodes = engine.snapshot();
    plico_dev::validate_nodes(&nodes).unwrap();
    let json = serde_json::to_string(&nodes).unwrap();
    let mut nodes: Vec<plico::ViewNode> = serde_json::from_str(&json).unwrap();
    assert_eq!(nodes[0].spans.len(), 1);
    nodes[0].spans[0].range = 1..3;
    assert!(plico_dev::validate_nodes(&nodes).is_err());
}
