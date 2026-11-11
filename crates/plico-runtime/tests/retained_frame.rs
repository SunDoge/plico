//! Native painting must not materialize a cloned placement snapshot.
use plico::prelude::*;
use plico_renderer::TextContext;
use plico_runtime::{HostView, Source};
#[derive(Default)]
struct Demo;
impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Count").width(160.).height(40.);
    }
}
#[test]
fn retained_window_path_paints_without_snapshot_clones() {
    let mut source = Source::new(Demo);
    let mut tcx = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    source
        .try_render_retained_frame(&mut scene, &mut tcx, (400, 300), 1.)
        .unwrap();
    assert_eq!(source.adapter.stats.placed_node_clones, 0);
    source.request_rebuild();
    source
        .try_render_retained_frame(&mut scene, &mut tcx, (400, 300), 1.)
        .unwrap();
    assert!(!scene.commands.is_empty());
    assert_eq!(source.adapter.stats.placed_node_clones, 0);
}
