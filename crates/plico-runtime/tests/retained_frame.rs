//! Native painting must not materialize a cloned placement snapshot.
use plico_core::prelude::*;
use plico_renderer::{FrameSource, TextContext};
use plico_runtime::{Source, View};
#[derive(Default)]
struct Demo;
impl View for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Count").width(160.).height(40.);
    }
}
#[test]
fn retained_window_path_paints_without_snapshot_clones() {
    let mut source = Source::new(Demo);
    let mut tcx = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    source.render_retained_frame(&mut scene, &mut tcx, (400, 300), 1.);
    assert!(source.last_error.is_none());
    assert_eq!(source.adapter.stats.placed_node_clones, 0);
    source.request_rebuild();
    source.render_retained_frame(&mut scene, &mut tcx, (400, 300), 1.);
    assert!(!scene.commands.is_empty());
    assert_eq!(source.adapter.stats.placed_node_clones, 0);
}
