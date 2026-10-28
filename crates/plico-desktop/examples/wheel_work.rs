//! CPU probe for real wheel events, including the shell's pre-event flush.
//! Run with `cargo run --release -p plico-desktop --example wheel_work`.
//! No window, GPU or present; timings are not end-to-end frame latency.
use plico::VirtualClock;
use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, Scene, TextContext, UiEvent, WheelDeltaMode,
    WheelEvent,
};
use std::time::{Duration, Instant};

struct Log {
    rows: Vec<String>,
    virtualized: bool,
    smooth: bool,
    calls: usize,
}
impl View for Log {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        if self.virtualized {
            virtual_list(cx, self.rows.len(), 24.).width(600.).height(360.).smooth_scroll(self.smooth).rows(
                |i| i,
                |cx, i| {
                    text(cx, &self.rows[i]);
                },
            );
        } else {
            column(cx).width(600.).height(360.).scroll_y(true).smooth_scroll(self.smooth) + |cx| {
                for (i, line) in self.rows.iter().enumerate() {
                    column(cx).key(i).height(24.) + |cx| {
                        text(cx, line);
                    };
                }
            };
        }
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
fn main() {
    for (virtualized, count) in [(false, 1000), (true, 100_000)] {
        for smooth in [false, true] {
            let clock = VirtualClock::default();
            let mut source = Source::with_clock(
                Log {
                    rows: (0..count)
                        .map(|i| format!("Log {i}: completed operation"))
                        .collect(),
                    virtualized,
                    smooth,
                    calls: 0,
                },
                clock.clone(),
            );
            let mut text = TextContext::new();
            let mut scene = Scene::new();
            source
                .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
                .unwrap();
            let mut samples = Vec::new();
            let mut calls = 0;
            let mut layouts = 0;
            for i in 0..240 {
                clock.advance(Duration::from_millis(16)).unwrap();
                let before = source.batch_stats();
                let start = Instant::now();
                source.prepare_for_event(&mut text);
                assert!(
                    source
                        .handle_event(wheel(if (i / 60) % 2 == 0 { 48. } else { -48. }))
                        .handled
                );
                source
                    .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
                    .unwrap();
                let elapsed = start.elapsed().as_nanos();
                assert!(source.last_error.is_none());
                if i >= 40 {
                    samples.push(elapsed);
                    calls += source.batch_stats().declaration_passes - before.declaration_passes;
                    layouts += source.batch_stats().layout_runs - before.layout_runs;
                }
            }
            samples.sort_unstable();
            println!(
                "virtual={virtualized} rows={count} smooth={smooth}: p50={}us p95={}us show/sample={:.2} layouts/sample={:.2}",
                samples[100] / 1000,
                samples[190] / 1000,
                calls as f64 / 200.,
                layouts as f64 / 200.
            );
            clock.advance(Duration::from_secs(1)).unwrap();
            source
                .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
                .unwrap();
            let calls = source.view.calls;
            source
                .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
                .unwrap();
            assert_eq!(source.view.calls, calls);
            assert!(!source.has_anim());
        }
    }
}
