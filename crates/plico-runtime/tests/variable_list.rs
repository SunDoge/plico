//! Variable rows learn their real Taffy geometry instead of requiring the app
//! to duplicate content height calculations.
use plico::prelude::*;
use plico::{Easing, VirtualClock};
use plico_renderer::{
    FrameSource, GesturePhase, Point, Scene, TextContext, UiEvent, WheelDeltaMode, WheelEvent,
};
use plico_runtime::{HostView, Source};
use std::{ops::Range, time::Duration};

struct Demo {
    base: usize,
    count: usize,
    follow_end: bool,
    reveal: Option<usize>,
    rendered: Range<usize>,
    shows: usize,
}

impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        let base = self.base;
        let list = variable_virtual_list(cx, self.count, 20.)
            .key("variable-list")
            .height(100.)
            .overscan(0)
            .reveal(self.reveal);
        let list = if self.follow_end {
            list.follow_end()
        } else {
            list
        };
        self.rendered = list
            .rows(
                |index| (base + index) as u64,
                |cx, index| {
                    column(cx)
                        .height(if index % 2 == 0 { 20. } else { 60. })
                        .shrink(0.)
                        .children(|cx| {
                            text(cx, format!("item {}", base + index));
                        });
                },
            )
            .rendered;
    }
}

#[test]
fn variable_list_redeclares_from_sparse_host_measurements() {
    let mut source = Source::new(Demo {
        base: 0,
        count: 1_000_000,
        follow_end: false,
        reveal: None,
        rendered: 0..0,
        shows: 0,
    });
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (300, 160), 1.)
        .unwrap();
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text, (300, 160), 1.)
            .unwrap();
    }
    assert_eq!(source.view().rendered, 0..4);
    assert!(source.view().shows >= 2);
    assert!(source.placed_nodes().len() < 32);
    let settled_shows = source.view().shows;
    source
        .render_retained_frame(&mut scene, &mut text, (300, 160), 1.)
        .unwrap();
    assert_eq!(source.view().shows, settled_shows);
}

#[test]
fn prepending_rows_keeps_the_anchor_item_at_the_same_visual_position() {
    let mut source = Source::new(Demo {
        base: 1_000,
        count: 200,
        follow_end: false,
        reveal: None,
        rendered: 0..0,
        shows: 0,
    });
    let mut text_context = TextContext::new();
    let mut scene = Scene::new();
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    assert!(
        source
            .handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x: 50., y: 50. },
                delta_x: 0.,
                delta_y: 400.,
                delta_mode: WheelDeltaMode::Pixel,
                phase: GesturePhase::Changed,
                modifiers: Default::default(),
            }))
            .handled
    );
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let anchor = source.view().base + source.view().rendered.start;
    let label = format!("item {anchor}");
    let before = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.text.as_deref() == Some(label.as_str()))
        .expect("anchor row is mounted")
        .rect[1];

    source.update(|view| {
        view.base -= 10;
        view.count += 10;
    });
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let after = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.text.as_deref() == Some(label.as_str()))
        .expect("the previous anchor follows its business key")
        .rect[1];
    assert!(
        (after - before).abs() < 0.5,
        "anchor jumped: {before} -> {after}"
    );
}

fn scroll_offset(source: &Source<Demo>) -> f32 {
    source
        .placed_nodes()
        .iter()
        .find(|node| node.scroll.scrollable[1])
        .expect("variable list scroll port")
        .scroll
        .offset[1]
}

#[test]
fn follow_end_tracks_growth_until_the_user_scrolls_away() {
    let mut source = Source::new(Demo {
        base: 0,
        count: 20,
        follow_end: true,
        reveal: None,
        rendered: 0..0,
        shows: 0,
    });
    let mut text_context = TextContext::new();
    let mut scene = Scene::new();
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let initial_end = scroll_offset(&source);
    assert!(initial_end > 500.);

    source.update(|view| view.count += 2);
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let appended_end = scroll_offset(&source);
    assert!(appended_end > initial_end);

    assert!(
        source
            .handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x: 50., y: 50. },
                delta_x: 0.,
                delta_y: -120.,
                delta_mode: WheelDeltaMode::Pixel,
                phase: GesturePhase::Changed,
                modifiers: Default::default(),
            }))
            .handled
    );
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let history_offset = scroll_offset(&source);
    assert!(history_offset < appended_end - 100.);

    source.update(|view| view.count += 2);
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    assert!(
        (scroll_offset(&source) - history_offset).abs() < 0.5,
        "append stole the user's history position"
    );

    assert!(
        source
            .handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x: 50., y: 50. },
                delta_x: 0.,
                delta_y: 10_000.,
                delta_mode: WheelDeltaMode::Pixel,
                phase: GesturePhase::Changed,
                modifiers: Default::default(),
            }))
            .handled
    );
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let resumed_end = scroll_offset(&source);
    assert!(resumed_end > history_offset);
    source.update(|view| view.count += 2);
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    assert!(scroll_offset(&source) > resumed_end);

    source.update(|view| view.reveal = Some(0));
    source
        .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
        .unwrap();
    source.update(|view| view.reveal = None);
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    assert!(scroll_offset(&source) < 0.5, "reveal lost to follow-end");
}

struct FixedDemo {
    base: usize,
    count: usize,
}

impl HostView for FixedDemo {
    fn show(&mut self, cx: &mut UiContext) {
        let base = self.base;
        virtual_list(cx, self.count, 20.)
            .key("fixed-list")
            .height(100.)
            .overscan(0)
            .follow_end()
            .rows(
                |index| (base + index) as u64,
                |cx, index| {
                    text(cx, format!("fixed {}", base + index));
                },
            );
    }
}

#[test]
fn fixed_list_follow_end_uses_the_same_user_scroll_contract() {
    let mut source = Source::new(FixedDemo { base: 0, count: 50 });
    let mut text_context = TextContext::new();
    let mut scene = Scene::new();
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let offset = |source: &Source<FixedDemo>| {
        source
            .placed_nodes()
            .iter()
            .find(|node| node.scroll.scrollable[1])
            .unwrap()
            .scroll
            .offset[1]
    };
    let initial_end = offset(&source);
    assert!((initial_end - 900.).abs() < 0.5);
    source.update(|view| view.count += 5);
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let appended_end = offset(&source);
    assert!((appended_end - 1_000.).abs() < 0.5);

    source.handle_event(UiEvent::Wheel(WheelEvent {
        position: Point { x: 50., y: 50. },
        delta_x: 0.,
        delta_y: -120.,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Default::default(),
    }));
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let history = offset(&source);
    source.update(|view| view.count += 5);
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    assert!((offset(&source) - history).abs() < 0.5);
}

#[test]
fn fixed_list_prepend_follows_the_anchor_business_key() {
    let mut source = Source::new(FixedDemo {
        base: 1_000,
        count: 200,
    });
    let mut text_context = TextContext::new();
    let mut scene = Scene::new();
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    source.handle_event(UiEvent::Wheel(WheelEvent {
        position: Point { x: 50., y: 50. },
        delta_x: 0.,
        delta_y: 400.,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Default::default(),
    }));
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let anchor = source
        .placed_nodes()
        .iter()
        .find(|node| {
            node.paint
                .text
                .as_deref()
                .is_some_and(|text| text.starts_with("fixed "))
        })
        .expect("anchor item mounted")
        .clone();
    let label = anchor.paint.text.as_deref().unwrap().to_owned();
    let before = anchor.rect[1];

    source.update(|view| {
        view.base -= 10;
        view.count += 10;
    });
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let after = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.text.as_deref() == Some(label.as_str()))
        .expect("anchor item follows its key")
        .rect[1];
    assert!(
        (after - before).abs() < 0.5,
        "anchor jumped: {before} -> {after}"
    );
}

struct SectionedFixedDemo {
    rendered: Range<usize>,
}

impl HostView for SectionedFixedDemo {
    fn show(&mut self, cx: &mut UiContext) {
        self.rendered = virtual_list(cx, 100, 20.)
            .key("sectioned-fixed-list")
            .height(100.)
            .overscan(0)
            .rows_with_headers(
                |index| index as u64,
                |index| index.is_multiple_of(10),
                |cx, index| {
                    text(
                        cx,
                        if index.is_multiple_of(10) {
                            format!("fixed section {index}")
                        } else {
                            format!("fixed row {index}")
                        },
                    );
                },
            )
            .rendered;
    }
}

#[test]
fn fixed_list_keeps_the_section_header_and_the_next_one_pushes_it() {
    let mut source = Source::new(SectionedFixedDemo { rendered: 0..0 });
    let mut text_context = TextContext::new();
    let mut scene = Scene::new();
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    source.handle_event(UiEvent::Wheel(WheelEvent {
        position: Point { x: 50., y: 50. },
        delta_x: 0.,
        delta_y: 390.,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Default::default(),
    }));
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    assert!(source.view().rendered.start > 10);
    let header = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.text.as_deref() == Some("fixed section 10"))
        .expect("the header above the virtual range stays mounted");
    let next = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.text.as_deref() == Some("fixed section 20"))
        .expect("the next visible header is mounted");
    let sticky: Vec<_> = source
        .placed_nodes()
        .iter()
        .filter(|node| node.paint.sticky_top.is_some())
        .collect();
    assert_eq!(sticky.len(), 2);
    assert!((sticky[0].rect[3] - sticky[1].rect[1]).abs() < 0.5);
    assert!(header.rect[1] < next.rect[1]);
    assert!(
        source.placed_nodes().len() < 40,
        "placed={} rendered={:?}",
        source.placed_nodes().len(),
        source.view().rendered
    );
}

struct SectionedVariableDemo {
    rendered: Range<usize>,
}

impl HostView for SectionedVariableDemo {
    fn show(&mut self, cx: &mut UiContext) {
        self.rendered = variable_virtual_list(cx, 100, 20.)
            .key("sectioned-variable-list")
            .height(100.)
            .overscan(0)
            .rows_with_headers(
                |index| index as u64,
                |index| index.is_multiple_of(10),
                |cx, index| {
                    column(cx)
                        .height(if index.is_multiple_of(10) { 28. } else { 20. })
                        .shrink(0.)
                        .children(|cx| {
                            text(
                                cx,
                                if index.is_multiple_of(10) {
                                    format!("variable section {index}")
                                } else {
                                    format!("variable row {index}")
                                },
                            );
                        });
                },
            )
            .rendered;
    }
}

#[test]
fn variable_list_measures_and_pins_the_header_above_its_virtual_range() {
    let mut source = Source::new(SectionedVariableDemo { rendered: 0..0 });
    let mut text_context = TextContext::new();
    let mut scene = Scene::new();
    for _ in 0..4 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    source.handle_event(UiEvent::Wheel(WheelEvent {
        position: Point { x: 50., y: 50. },
        delta_x: 0.,
        delta_y: 300.,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Default::default(),
    }));
    for _ in 0..5 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    let first = source.view().rendered.start;
    assert!(first > 10);
    let section = first / 10 * 10;
    assert!(section < first, "test must exercise an off-range header");
    let label = format!("variable section {section}");
    let header = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.text.as_deref() == Some(label.as_str()))
        .expect("the measured header above the virtual range stays mounted");
    let port_top = source
        .placed_nodes()
        .iter()
        .find(|node| node.scroll.scrollable[1])
        .unwrap()
        .scroll
        .port[1];
    assert!((header.rect[1] - port_top).abs() < 0.5);
    assert!(
        source.placed_nodes().len() < 40,
        "placed={} rendered={:?}",
        source.placed_nodes().len(),
        source.view().rendered
    );
}

struct AnimatedFixedDemo {
    count: usize,
}

impl HostView for AnimatedFixedDemo {
    fn show(&mut self, cx: &mut UiContext) {
        virtual_list(cx, self.count, 20.)
            .key("animated-fixed-list")
            .height(100.)
            .overscan(0)
            .follow_end_with(Animation::tween(Duration::from_millis(200), Easing::Linear))
            .rows(
                |index| index as u64,
                |cx, index| {
                    text(cx, format!("animated {index}"));
                },
            );
    }
}

#[test]
fn animated_follow_end_uses_the_injected_clock_and_retargets_continuously() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(AnimatedFixedDemo { count: 50 }, clock.clone());
    let mut text_context = TextContext::new();
    let mut scene = Scene::new();
    let offset = |source: &Source<AnimatedFixedDemo>| {
        source
            .placed_nodes()
            .iter()
            .find(|node| node.scroll.scrollable[1])
            .unwrap()
            .scroll
            .offset[1]
    };
    for _ in 0..3 {
        source
            .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
            .unwrap();
    }
    assert!((offset(&source) - 900.).abs() < 0.5);

    source.update(|view| view.count += 5);
    source
        .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
        .unwrap();
    assert!((offset(&source) - 900.).abs() < 0.5);
    clock.advance(Duration::from_millis(100)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
        .unwrap();
    let halfway = offset(&source);
    assert!((halfway - 950.).abs() < 1., "halfway offset: {halfway}");

    source.update(|view| view.count += 5);
    source
        .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
        .unwrap();
    assert!((offset(&source) - halfway).abs() < 1.);
    clock.advance(Duration::from_millis(200)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text_context, (300, 160), 1.)
        .unwrap();
    let finished = offset(&source);
    assert!(
        (finished - 1_100.).abs() < 0.5,
        "finished offset: {finished}"
    );
}
