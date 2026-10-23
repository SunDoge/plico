//! Retained backend work with/without an application rebuild. CPU only.
use plico_backend::{Source, View};
use plico_core::{Color, UiContext, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::{Duration, Instant};
struct Demo {
    changed: bool,
    calls: usize,
    animate: bool,
}
impl View for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        cx.column().children(|cx| {
            for i in 0..1000 {
                cx.text("Retained paragraph")
                    .key(i as u64)
                    .color(if i == 0 && self.changed && !self.animate {
                        Color::Literal(0xff0000)
                    } else {
                        Color::Literal(0x222222)
                    })
                    .opacity(if self.animate && self.changed && i == 0 {
                        0.2
                    } else {
                        1.
                    })
                    .transition(if self.animate { 10_000 } else { 0 });
            }
        });
    }
}
fn main() {
    println!(
        "mode,total_p50_us,total_p95_us,show_calls,sync_visits_per_frame,layout_runs_per_frame,flatten_visits_per_frame,scene_builds_per_frame,scene_reuses_per_frame"
    );
    for mode in [
        "direct_unchanged",
        "forced_unchanged",
        "one_color_change",
        "host_opacity",
    ] {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(
            Demo {
                changed: false,
                calls: 0,
                animate: mode == "host_opacity",
            },
            clock.clone(),
        );
        let mut text = TextContext::new();
        let mut scene = Scene::new();
        let mut samples = Vec::new();
        let mut sums = [0usize; 5];
        let mut before_calls = 0;
        for frame in 0..136 {
            if frame == 16 {
                before_calls = source.view.calls;
            }
            clock.advance(Duration::from_millis(16)).unwrap();
            if mode == "host_opacity" && frame == 1 {
                source.view.changed = true;
                source.request_rebuild();
            }
            if matches!(mode, "forced_unchanged" | "one_color_change") {
                source.view.changed = mode == "one_color_change" && frame % 2 == 0;
                source.request_rebuild();
            }
            let start = Instant::now();
            source
                .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
                .unwrap();
            let us = start.elapsed().as_secs_f64() * 1e6;
            assert!(source.last_error.is_none());
            if frame >= 16 {
                samples.push(us);
                let s = source.adapter.stats;
                for (sum, value) in sums.iter_mut().zip([
                    s.sync_visits,
                    s.layout_runs,
                    s.flatten_visits,
                    s.scene_builds,
                    s.scene_reuses,
                ]) {
                    *sum += value;
                }
            }
        }
        samples.sort_by(f64::total_cmp);
        let calls = source.view.calls - before_calls;
        assert_eq!(
            calls,
            if matches!(mode, "host_opacity" | "direct_unchanged") {
                0
            } else {
                120
            }
        );
        assert_eq!(sums[1], 0);
        if matches!(mode, "direct_unchanged" | "forced_unchanged") {
            assert_eq!(sums[0], 0);
            assert_eq!(sums[3], 0);
        }
        println!(
            "{mode},{:.3},{:.3},{calls},{:.1},{:.1},{:.1},{:.1},{:.1}",
            samples[60],
            samples[114],
            sums[0] as f64 / 120.,
            sums[1] as f64 / 120.,
            sums[2] as f64 / 120.,
            sums[3] as f64 / 120.,
            sums[4] as f64 / 120.
        );
    }
}
