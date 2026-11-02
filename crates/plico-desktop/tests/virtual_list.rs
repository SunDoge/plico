use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, TextContext, UiEvent, WheelDeltaMode, WheelEvent,
};

struct Demo {
    count: usize,
    built: Vec<usize>,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.built.clear();
        virtual_list(cx, self.count, 40.)
            .key("list")
            .width(240.)
            .height(180.)
            .rows(
                |i| i as u64,
                |cx, i| {
                    self.built.push(i);
                    button(cx)
                        .text(format!("Item {i}"))
                        .height(40.)
                        .motion(Motion::None);
                },
            );
    }
}
fn wheel(delta: f64) -> UiEvent {
    UiEvent::Wheel(WheelEvent {
        position: Point { x: 20., y: 20. },
        delta_x: 0.,
        delta_y: delta,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Modifiers::empty(),
    })
}
#[test]
fn large_list_scrolls_to_real_rows_and_clamps_after_shrink() {
    let mut source = Source::new(Demo {
        count: 10_000,
        built: vec![],
    });
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.built, (0..7).collect::<Vec<_>>());
    // A composable button includes its row, control and label. Bound the
    // retained projection by mounted rows rather than the old leaf-button count.
    assert!(source.adapter.placed.len() < source.view.built.len() * 4 + 8);
    let port = source
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.scrollable[1])
        .unwrap();
    assert_eq!(port.scroll.range[1], 399_820.);
    assert!(source.handle_event(wheel(4000.)).handled);
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.built, (98..107).collect::<Vec<_>>());
    let row = source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Item 100"))
        .unwrap();
    // The label is centered inside a composable button; test the row container.
    let parent = source
        .adapter
        .placed
        .iter()
        .find(|p| Some(p.node_id) == row.parent_node_id)
        .unwrap();
    assert_eq!(parent.rect[1], 0.);
    // A composable button includes its row, control and label. Bound the
    // retained projection by mounted rows rather than the old leaf-button count.
    assert!(source.adapter.placed.len() < source.view.built.len() * 4 + 8);
    assert!(source.handle_event(wheel(1_000_000.)).handled);
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.built.last(), Some(&9999));
    source.view.count = 1;
    source.request_rebuild();
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.built, [0]);
    let port = source
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.port[3] == 180. && p.parent_node_id.is_some())
        .unwrap();
    assert_eq!(port.scroll.offset[1], 0.);
    source.view.count = 0;
    source.request_rebuild();
    source.build_frame(&mut tcx, 400, 300);
    assert!(source.view.built.is_empty());
}

#[test]
fn overlapping_rows_keep_entity_identity() {
    let mut source = Source::new(Demo {
        count: 10_000,
        built: vec![],
    });
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 400, 300);
    let before = source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Item 3"))
        .unwrap()
        .node_id;
    assert!(source.handle_event(wheel(80.)).handled);
    source.build_frame(&mut tcx, 400, 300);
    let after = source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Item 3"))
        .unwrap()
        .node_id;
    assert_eq!(before, after);
}

struct FillDemo {
    count: usize,
    built: Vec<usize>,
}
impl HostView for FillDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.built.clear();
        column(cx)
            .width_percent(100.)
            .height_percent(100.)
            .children(|cx| {
                virtual_list(cx, self.count, 40.).key("list").fill().rows(
                    |i| i as u64,
                    |cx, i| {
                        self.built.push(i);
                        text(cx, format!("Item {i}"));
                    },
                );
            });
    }
}
#[test]
fn fill_mode_windows_from_the_observed_viewport_and_tracks_resize() {
    let mut source = Source::new(FillDemo {
        count: 10_000,
        built: vec![],
    });
    let mut tcx = TextContext::new();
    // First frame windows from the fallback height; the host then reports the
    // real viewport and the next declaration re-windows from it.
    source.build_frame(&mut tcx, 400, 300);
    source.build_frame(&mut tcx, 400, 300);
    let port = source
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.scrollable[1])
        .unwrap();
    assert_eq!(port.scroll.port[3] - port.scroll.port[1], 300.);
    // 300 px of 40 px rows plus overscan renders a full viewport of rows.
    assert!(source.view.built.last().is_some_and(|&i| i >= 9));
    assert!(source.view.built.len() < 20);
    // Growing the window re-windows without re-declaring row identity.
    let before = source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Item 3"))
        .unwrap()
        .node_id;
    source.build_frame(&mut tcx, 400, 500);
    let after = source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Item 3"))
        .unwrap()
        .node_id;
    assert_eq!(before, after);
    let port = source
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.scrollable[1])
        .unwrap();
    assert_eq!(port.scroll.port[3] - port.scroll.port[1], 500.);
    // The resize frame still windows from the previously observed height; the
    // reported viewport re-declares on the following frame.
    source.build_frame(&mut tcx, 400, 500);
    assert!(
        source.view.built.len() > 10,
        "{:?}",
        source.view.built.len()
    );
    // Wheel scrolling still moves the window in fill mode.
    assert!(source.handle_event(wheel(400.)).handled);
    source.build_frame(&mut tcx, 400, 500);
    assert!(source.view.built.first().is_some_and(|&i| i > 0));
}
