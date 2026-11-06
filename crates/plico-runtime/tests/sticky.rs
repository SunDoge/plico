//! Sticky headers: the top edge stays in the scroll port until its parent
//! pushes it away.
use plico::prelude::*;
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, TextContext, UiEvent, WheelDeltaMode, WheelEvent,
};
use plico_runtime::{HostView, Source};

struct Demo;

impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        let _ = column(cx).width(300.).height(200.).scroll_y(true)
            + |cx| {
                let _ = column(cx)
                    .sticky(8.)
                    .height(30.)
                    .shrink(0.)
                    .width_percent(100.)
                    .background(Color::Literal(0x334455))
                    + |cx| {
                        text(cx, "Header")
                            .font_size(16)
                            .color(Color::Literal(0xffffff));
                    };
                for i in 0..20 {
                    let _ = row(cx).height(30.).shrink(0.).width_percent(100.)
                        + |cx| {
                            text!(cx, "Row {i}");
                        };
                }
            };
    }
}

fn wheel(y: f64) -> UiEvent {
    UiEvent::Wheel(WheelEvent {
        position: Point { x: 150., y },
        delta_x: 0.,
        delta_y: 100.,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Modifiers::empty(),
    })
}

fn placed_y(source: &Source<Demo>, sticky: bool) -> f32 {
    source
        .adapter
        .placed
        .iter()
        .find(|node| {
            if sticky {
                node.paint.sticky_top == Some(8.)
            } else {
                node.paint.text.as_deref() == Some("Header")
            }
        })
        .map(|node| node.rect[1])
        .expect("the sticky header and its text are placed")
}

#[test]
fn header_sticks_to_the_port_top_within_its_parent() {
    let mut source = Source::new(Demo);
    let mut text = TextContext::new();
    source.build_frame(&mut text, 300, 200);
    assert!((placed_y(&source, true) - 8.).abs() < 0.1);
    let text_before = placed_y(&source, false);

    source.handle_event(wheel(100.));
    source.build_frame(&mut text, 300, 200);
    assert!(
        (placed_y(&source, true) - 8.).abs() < 0.1,
        "sticks while scrolling"
    );
    assert!(
        (placed_y(&source, false) - text_before).abs() < 0.1,
        "the header's children move with it"
    );

    source.handle_event(wheel(100.));
    source.handle_event(wheel(100.));
    source.handle_event(wheel(100.));
    source.handle_event(wheel(100.));
    source.handle_event(wheel(100.));
    source.build_frame(&mut text, 300, 200);
    let header = source
        .adapter
        .placed
        .iter()
        .find(|node| node.paint.sticky_top == Some(8.))
        .unwrap();
    let parent = source
        .adapter
        .placed
        .iter()
        .find(|node| node.node_id == header.parent_node_id.unwrap())
        .unwrap();
    assert!(header.rect[1] >= 0.);
    assert!(
        header.rect[3] <= parent.rect[3] + 0.1,
        "the parent pushes the header away at the end"
    );
}
