//! Scrolling and unrelated layout animation must be committed in the same frame.
use plico::VirtualClock;
use plico::prelude::*;
use plico_renderer::{FrameSource, Point, Scene, TextContext, UiEvent, WheelEvent};
use plico_runtime::{HostView, Source};
use std::time::Duration;

struct Demo {
    height: f32,
    host_motion: bool,
    shows: usize,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        row(cx).width(400.).height(200.).children(|cx| {
            column(cx)
                .width(200.)
                .height(200.)
                .scroll_y(true)
                .children(|cx| {
                    column(cx)
                        .width(200.)
                        .height(1000.)
                        .shrink(0.)
                        .background(Color::Panel)
                        .children(|_| {});
                });
            let panel = column(cx)
                .width(100.)
                .height(self.height)
                .background(Color::Accent);
            let panel = if self.host_motion {
                panel.motion(Motion::Smooth)
            } else {
                panel
            };
            panel.children(|_| {});
        });
    }
}

#[test]
fn smooth_scroll_advances_during_unrelated_layout_changes() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            height: 20.,
            host_motion: false,
            shows: 0,
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    assert!(
        source
            .handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x: 50., y: 50. },
                delta_x: 0.,
                delta_y: 120.,
                delta_mode: plico_renderer::WheelDeltaMode::Line,
                phase: plico_renderer::GesturePhase::Changed,
                modifiers: Default::default(),
            }))
            .handled
    );
    let mut previous = 0.;
    for step in 1..=10 {
        clock.advance(Duration::from_millis(16)).unwrap();
        source.update(|view| view.height = 20. + step as f32);
        source
            .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
            .unwrap();
        let port = source
            .placed_nodes()
            .iter()
            .find(|node| node.scroll.scrollable[1])
            .unwrap();
        assert!(
            port.scroll.offset[1] > previous,
            "scroll froze at step {step}: {}",
            port.scroll.offset[1]
        );
        let content = source
            .placed_nodes()
            .iter()
            .find(|node| node.rect[3] - node.rect[1] > 900.)
            .unwrap();
        assert!((content.rect[1] - port.rect[1] + port.scroll.offset[1]).abs() < 0.01);
        previous = port.scroll.offset[1];
    }
    assert!((previous - 120.).abs() < 0.01);
}

#[test]
fn host_layout_animation_and_scroll_share_clock_without_redeclaring_each_frame() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            height: 20.,
            host_motion: true,
            shows: 0,
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    source.update(|view| view.height = 100.);
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    assert!(
        source
            .handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x: 50., y: 50. },
                delta_x: 0.,
                delta_y: 120.,
                delta_mode: plico_renderer::WheelDeltaMode::Line,
                phase: plico_renderer::GesturePhase::Changed,
                modifiers: Default::default(),
            }))
            .handled
    );
    let shows = source.view().shows;
    let mut previous = 0.;
    for step in 1..=10 {
        clock.advance(Duration::from_millis(16)).unwrap();
        source
            .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
            .unwrap();
        assert_eq!(
            source.view().shows,
            shows,
            "host animation should retain declarations"
        );
        if step == 1 {
            assert_eq!(
                source.frame_diagnostics().backend.layout_runs,
                1,
                "the regression must include an actual layout animation"
            );
        }
        let port = source
            .placed_nodes()
            .iter()
            .find(|node| node.scroll.scrollable[1])
            .unwrap();
        assert!(
            port.scroll.offset[1] > previous,
            "host motion froze scroll at step {step}"
        );
        previous = port.scroll.offset[1];
    }
    assert!((previous - 120.).abs() < 0.01);
}
