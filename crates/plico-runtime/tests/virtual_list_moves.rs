use plico::prelude::*;
use plico::{Easing, VirtualClock};
use plico_renderer::{
    FrameSource, GesturePhase, Point, Scene, TextContext, UiEvent, WheelDeltaMode, WheelEvent,
    WindowMetrics,
};
use plico_runtime::{HostView, Source};
use std::{ops::Range, time::Duration};

const DURATION: Duration = Duration::from_millis(200);

struct List {
    leading: Vec<usize>,
    variable: bool,
    mixed: bool,
    reveal: Option<usize>,
    follow: bool,
    rendered: Range<usize>,
}

impl HostView for List {
    fn show(&mut self, cx: &mut UiContext) {
        let count = self.leading.len() + if self.follow { 0 } else { 1_000_000 };
        let animation = Animation::tween(DURATION, Easing::Linear);
        let key = |i: usize| {
            self.leading
                .get(i)
                .copied()
                .unwrap_or_else(|| i - self.leading.len() + 100)
        };
        let row = |cx: &mut UiContext<'_>, i| {
            let id = key(i);
            column(cx)
                .height(if self.mixed && id.is_multiple_of(2) {
                    30.
                } else {
                    20.
                })
                .shrink(0.)
                .children(|cx| {
                    text(cx, format!("record {id}"));
                });
        };
        self.rendered = if self.variable {
            variable_virtual_list(cx, count, 20.)
                .height(101.)
                .overscan(0)
                .reveal(self.reveal)
                .move_animation(animation)
                .rows_with_headers(key, |i| key(i) == 0, row)
        } else {
            let list = virtual_list(cx, count, 20.)
                .height(101.)
                .overscan(0)
                .reveal(self.reveal)
                .move_animation(animation);
            let list = if self.follow {
                list.follow_end_with(animation)
            } else {
                list
            };
            list.rows_with_headers(key, |i| key(i) == 0, row)
        }
        .rendered;
    }
}

struct TestList {
    source: Source<List>,
    clock: VirtualClock,
    scene: Scene,
    text: TextContext,
}

impl TestList {
    fn new(variable: bool, mixed: bool) -> Self {
        let clock = VirtualClock::default();
        let mut list = Self {
            source: Source::with_clock(
                List {
                    leading: (0..12).collect(),
                    variable,
                    mixed,
                    reveal: None,
                    follow: false,
                    rendered: 0..0,
                },
                clock.clone(),
            ),
            clock,
            scene: Scene::new(),
            text: TextContext::new(),
        };
        for _ in 0..4 {
            list.frame();
        }
        list
    }

    fn frame(&mut self) {
        self.source
            .render_frame(&mut self.scene, &mut self.text, (300, 140), 1.)
            .unwrap();
    }

    fn y(&self, id: usize) -> f32 {
        let label = format!("record {id}");
        let node = self
            .source
            .placed_nodes()
            .iter()
            .find(|node| node.paint.text.as_deref() == Some(&label))
            .unwrap_or_else(|| panic!("missing {label}"));
        self.source.visual_bounds(node.node_id).unwrap()[1]
    }

    fn assert_y(&self, id: usize, expected: f32) {
        let y = self.y(id);
        assert!(
            (y - expected).abs() < 0.6,
            "row {id}: {y}, expected {expected}"
        );
    }
}

#[test]
fn row_moves_retarget_from_the_presented_position() {
    for variable in [false, true] {
        let mut list = TestList::new(variable, variable);
        let original = list.y(1);
        let destination = original + (if variable { 30. } else { 20. });
        list.source.update(|view| view.leading.swap(1, 2));
        list.frame();
        list.assert_y(1, original);
        list.clock.advance(DURATION / 2).unwrap();
        list.frame();
        let halfway = (original + destination) / 2.;
        list.assert_y(1, halfway);
        assert_eq!(list.source.frame_diagnostics().backend.layout_runs, 0);

        list.source.update(|view| view.leading.swap(1, 2));
        list.frame();
        list.assert_y(1, halfway);
        list.frame();
        list.assert_y(1, halfway);
        list.clock.advance(DURATION).unwrap();
        list.frame();
        list.assert_y(1, original);
    }
}

#[test]
fn insert_and_remove_cover_the_exposed_edge_without_building_the_full_list() {
    for variable in [false, true] {
        let mut list = TestList::new(variable, false);
        let original = list.y(2);
        let normal_range = list.source.view().rendered.clone();
        list.source.update(|view| {
            view.leading.insert(2, 99);
        });
        list.frame();
        list.assert_y(2, original);
        list.assert_y(99, original);
        assert!(list.source.view().rendered.end > normal_range.end);
        assert!(list.source.placed_nodes().len() < 36);
        list.clock.advance(DURATION).unwrap();
        list.frame();
        list.assert_y(2, original + 20.);

        list.source.update(|view| {
            view.leading.remove(2);
        });
        list.frame();
        list.assert_y(2, original + 20.);
        list.clock.advance(DURATION).unwrap();
        list.frame();
        list.assert_y(2, original);
        assert_eq!(list.source.view().rendered, normal_range);
    }
}

#[test]
fn reveal_scroll_headers_and_reduced_motion_take_precedence() {
    let mut list = TestList::new(false, false);
    let header = list.y(0);
    let before_scroll = list.y(2);
    list.source.handle_event(UiEvent::Wheel(WheelEvent {
        position: Point { x: 50., y: 50. },
        delta_x: 0.,
        delta_y: 20.,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Default::default(),
    }));
    list.frame();
    list.clock.advance(DURATION).unwrap();
    list.frame();
    list.assert_y(0, header);
    list.assert_y(2, before_scroll - 20.);
    list.source.update(|view| {
        view.leading.swap(3, 4);
        view.reveal = Some(9);
    });
    list.frame();
    list.assert_y(9, header + 81.);
    list.assert_y(0, header);

    list.source
        .handle_event(UiEvent::WindowMetrics(WindowMetrics {
            reduced_motion: true,
            ..Default::default()
        }));
    list.source.update(|view| {
        view.reveal = None;
        view.leading.swap(7, 8);
    });
    list.frame();
    list.assert_y(7, header + 61.);
}

#[test]
fn following_the_end_owns_row_placement_during_append() {
    let mut list = TestList::new(false, false);
    list.source.update(|view| view.follow = true);
    list.frame();
    let before = list.y(10);
    list.source.update(|view| view.leading.push(12));
    list.frame();
    list.assert_y(10, before);
    list.clock.advance(DURATION / 2).unwrap();
    list.frame();
    list.assert_y(10, before - 10.);
    list.clock.advance(DURATION / 2).unwrap();
    list.frame();
    list.assert_y(10, before - 20.);
}
