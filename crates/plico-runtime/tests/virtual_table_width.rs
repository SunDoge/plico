//! Zero-basis table cells need a concrete, full-width virtual viewport and row.
use plico::prelude::*;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{HostView, Source};
struct Table;
impl HostView for Table {
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height(300.).children(|cx| {
            text(cx, "Header");
            virtual_list(cx, 100, 40.)
                .width_percent(100.)
                .height(200.)
                .rows(
                    |i| i as u64,
                    |cx, i| {
                        row(cx)
                            .width_percent(100.)
                            .height(40.)
                            .padding(10)
                            .children(|cx| {
                                text(cx, "Name").width(0.).grow(2.);
                                text(
                                    cx,
                                    format!(
                                        "{} {}",
                                        if i % 3 == 0 { "Translated" } else { "Ready" },
                                        i
                                    ),
                                )
                                .width(0.)
                                .grow(1.);
                            });
                    },
                );
        });
    }
}
#[test]
fn virtual_rows_and_flex_cells_follow_viewport_width_after_resize() {
    let mut source = Source::new(Table);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    for width in [400, 640, 300] {
        source
            .render_retained_frame(&mut scene, &mut text, (width, 300), 1.)
            .unwrap();
        let port = source
            .adapter
            .placed
            .iter()
            .find(|node| node.scroll.scrollable[1])
            .unwrap();
        assert!((port.rect[2] - port.rect[0] - width as f32).abs() < 1.);
        let name = source
            .adapter
            .placed
            .iter()
            .find(|node| node.paint.text.as_deref() == Some("Name"))
            .unwrap();
        let status = source
            .adapter
            .placed
            .iter()
            .find(|node| node.paint.text.as_deref() == Some("Translated 0"))
            .unwrap();
        let name_width = name.rect[2] - name.rect[0];
        let status_width = status.rect[2] - status.rect[0];
        assert!(
            name_width > 100. && status_width > 60.,
            "cells collapsed: {name_width}, {status_width}"
        );
        assert!((name_width / status_width - 2.).abs() < 0.05);
        for cell in source.adapter.placed.iter().filter(|node| {
            node.paint
                .text
                .as_deref()
                .is_some_and(|text| text.starts_with("Ready ") || text.starts_with("Translated "))
        }) {
            assert!(
                (cell.rect[0] - status.rect[0]).abs() < 1.,
                "status columns misaligned: {:?} != {:?}",
                cell.rect,
                status.rect
            );
        }
    }
}
