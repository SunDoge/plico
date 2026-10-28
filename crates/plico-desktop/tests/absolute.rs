use plico::Style;
use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, TextContext};
struct Demo {
    x: f32,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width(240.).gap(10).children(|cx| {
            column(cx).width(240.).height(120.).children(|cx| {
                column(cx)
                    .absolute(self.x, 20.)
                    .width(40.)
                    .height(40.)
                    .children(|cx| {
                        text(cx, "Tile");
                    })
            });
            text(cx, "After");
        })
    }
}
#[test]
fn absolute_nodes_are_local_and_position_changes_invalidate_layout() {
    let mut source = Source::new(Demo { x: 30. });
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 400, 300);
    let before = source
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Tile"))
        .unwrap()
        .clone();
    let after_y = source
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("After"))
        .unwrap()
        .rect[1];
    assert_eq!(before.rect[0], 30.);
    assert_eq!(before.rect[1], 20.);
    assert_eq!(after_y, 130.);
    source.view.x = -10.;
    source.build_frame(&mut tcx, 400, 300);
    let moved = source
        .adapter
        .placed
        .iter()
        .find(|n| n.node_id == before.node_id)
        .unwrap();
    assert_eq!(moved.rect[0], -10.);
    assert!(source.adapter.stats.layout_runs > 0);
    assert_eq!(source.adapter.stats.created, 0);
    assert!(
        !Style {
            absolute: Some([f32::NAN, 0.]),
            ..Default::default()
        }
        .valid()
    );
    assert!(
        Style {
            absolute: Some([-5., 0.]),
            ..Default::default()
        }
        .valid()
    );
}

#[test]
fn simulated_layout_excludes_absolute_children_and_updates_when_flow_changes() {
    let mut engine = plico::Engine::default();
    for (absolute, height) in [(false, 90.), (true, 30.), (false, 90.)] {
        engine
            .frame(|cx| {
                let _ = column(cx).key("parent").width(200.).gap(10).children(|cx| {
                    column(cx).height(30.).children(|_cx| {});
                    let tile = column(cx).height(50.);
                    let tile = if absolute {
                        tile.absolute(0., 0.)
                    } else {
                        tile
                    };
                    let _ = tile.children(|_cx| {});
                });
            })
            .unwrap();
        let parent = engine.find(engine.root(), "parent").unwrap();
        assert_eq!(engine.node(parent).unwrap().size.height, height);
    }
}
