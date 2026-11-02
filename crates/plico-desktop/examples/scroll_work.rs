//! CPU-only scroll declaration experiment. No window, GPU submission or sleeps.
use plico::{VirtualClock, prelude::*};
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, Scene, TextContext, UiEvent, WheelDeltaMode,
    WheelEvent,
};
use std::time::{Duration, Instant};
struct Demo {
    labels: Vec<String>,
    observe: bool,
    calls: usize,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        column(cx)
            .width(300.)
            .height(240.)
            .scroll_y(true)
            .smooth_scroll(true)
            + |cx| {
                if self.observe {
                    let _ = cx.scroll_viewport();
                }
                for (id, label) in self.labels.iter().enumerate() {
                    column(cx).key(id).height(24.).shrink(0.)
                        + |cx| {
                            text(cx, label.as_str()).height(24.);
                        };
                }
            };
    }
}
fn main() {
    println!(
        "mode,p50_us,p95_us,show_calls_per_sample,layout_runs_per_sample,flatten_visits_per_sample"
    );
    for mode in ["presentation", "forced_declaration", "observed_scroll"] {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(
            Demo {
                labels: (0..1000).map(|i| format!("Row {i}")).collect(),
                observe: mode == "observed_scroll",
                calls: 0,
            },
            clock.clone(),
        );
        let mut text = TextContext::new();
        let mut scene = Scene::new();
        source
            .render_retained_frame(&mut scene, &mut text, (600, 400), 1.)
            .unwrap();
        let mut times = Vec::new();
        let mut calls = 0;
        let mut layouts = 0;
        let mut flattened = 0;
        for cycle in 0..22 {
            assert!(
                source
                    .handle_event(UiEvent::Wheel(WheelEvent {
                        position: Point { x: 20., y: 20. },
                        delta_x: 0.,
                        delta_y: if cycle % 2 == 0 { 480. } else { -480. },
                        delta_mode: WheelDeltaMode::Pixel,
                        phase: GesturePhase::Changed,
                        modifiers: Modifiers::empty(),
                    }))
                    .handled
            );
            source
                .render_retained_frame(&mut scene, &mut text, (600, 400), 1.)
                .unwrap();
            for _ in 0..10 {
                clock.advance(Duration::from_millis(16)).unwrap();
                if mode == "forced_declaration" {
                    source.request_rebuild();
                }
                let before = source.view.calls;
                let start = Instant::now();
                source
                    .render_retained_frame(&mut scene, &mut text, (600, 400), 1.)
                    .unwrap();
                let us = start.elapsed().as_secs_f64() * 1e6;
                assert!(source.last_error.is_none());
                assert_eq!(
                    source.view.calls - before,
                    usize::from(mode != "presentation")
                );
                assert_eq!(source.adapter.stats.layout_runs, 0);
                if cycle >= 2 {
                    times.push(us);
                    calls += source.view.calls - before;
                    layouts += source.adapter.stats.layout_runs;
                    flattened += source.adapter.stats.flatten_visits;
                }
            }
            assert!(!source.has_anim());
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{mode},{:.3},{:.3},{:.1},{:.1},{:.1}",
            times[100],
            times[190],
            calls as f64 / 200.,
            layouts as f64 / 200.,
            flattened as f64 / 200.
        );
    }
}
