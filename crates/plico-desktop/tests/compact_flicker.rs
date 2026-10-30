//! Frame-by-frame inspection of a compact-metrics toggle: consecutive
//! rendered scenes must never present an intermediate state (sizes from the
//! old metrics with paint from the new, or vice versa).
use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{FrameSource, TextContext};
use std::sync::Arc;
use std::time::Duration;

struct App {
    compact: bool,
}
impl HostView for App {
    fn theme(&self) -> plico::Theme {
        let mut theme = plico::Theme::light();
        if self.compact {
            theme.metrics = plico::Metrics::compact();
        }
        theme
    }
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).gap(10.)
            + |cx| {
                text(cx, "Regular label").font_size(16);
                text(cx, "Second label").font_size(16);
                if self.compact {
                    text(cx, "Only when compact").font_size(16);
                }
            };
    }
}

#[test]
fn compact_toggle_is_atomic_across_frames() {
    let clock = Arc::new(plico::VirtualClock::default());
    let mut s = Source::with_clock(App { compact: false }, clock.clone());
    let mut tcx = TextContext::new();
    let before = s.build_frame(&mut tcx, 400, 300);
    let before_text: Vec<_> = before.iter().filter_map(|n| n.paint.text.clone()).collect();
    // Toggle and advance one tick, like a vsync-driven redraw would.
    s.view.compact = true;
    s.request_rebuild();
    clock.advance(Duration::from_millis(16)).unwrap();
    let after = s.build_frame(&mut tcx, 400, 300);
    let after_text: Vec<_> = after.iter().filter_map(|n| n.paint.text.clone()).collect();
    // The new node must appear in the SAME frame as the metric change — no
    // intermediate frame with old layout + new text set.
    assert!(after_text.iter().any(|t| &**t == "Only when compact"));
    assert_eq!(before_text.len(), 2);
    assert_eq!(after_text.len(), 3);
    // And geometry must be self-consistent: every placed node has positive
    // size (no zero-height stragglers from a half-applied metrics change).
    for n in &after {
        assert!(n.rect[2] > n.rect[0], "{:?}", n.rect);
        assert!(n.rect[3] > n.rect[1], "{:?}", n.rect);
    }
    // A second frame must be pixel-stable (no continuing churn).
    let settled = s.build_frame(&mut tcx, 400, 300);
    assert_eq!(
        after_text,
        settled
            .iter()
            .filter_map(|n| n.paint.text.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn compact_toggle_settles_within_one_frame() {
    let clock = Arc::new(plico::VirtualClock::default());
    let mut s = Source::with_clock(App { compact: false }, clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    s.view.compact = true;
    s.request_rebuild();
    // Advance several vsyncs; after the first, every subsequent frame must
    // be identical (scene bytes AND stats) — continuing churn is what reads
    // as flicker.
    let mut previous = None;
    let mut churn = 0;
    for tick in 0..6 {
        clock.advance(Duration::from_millis(16)).unwrap();
        let mut scene = plico_renderer::Scene::new();
        let nodes = s.build_frame(&mut tcx, 400, 300);
        plico_renderer::scene::build_scene(&mut scene, &nodes, &mut tcx, 400, 300, s.base_color());
        let stats = s.adapter.stats;
        if tick > 0 {
            // After the toggle frame, stats must be all-zero (steady state).
            if stats.paint_writes > 0 || stats.style_writes > 0 || stats.flatten_visits > 0 {
                churn += 1;
                eprintln!(
                    "tick {tick}: paint={} style={} flatten={} layout={}",
                    stats.paint_writes, stats.style_writes, stats.flatten_visits, stats.layout_runs
                );
            }
        }
        if let Some(previous) = &previous {
            assert_eq!(
                &scene.commands, previous,
                "tick {tick}: scene changed after settling"
            );
        }
        previous = Some(scene.commands.clone());
    }
    assert_eq!(churn, 0, "backend keeps rewriting after the compact toggle");
}

#[test]
fn compact_toggle_is_atomic_on_the_host_paint_path() {
    let clock = Arc::new(plico::VirtualClock::default());
    let mut s = Source::with_clock(App { compact: false }, clock.clone());
    let mut tcx = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    s.build_and_paint(&mut scene, &mut tcx, 400, 300, 1.);
    s.view.compact = true;
    s.request_rebuild();
    let mut snapshots = Vec::new();
    for _ in 0..4 {
        clock.advance(Duration::from_millis(16)).unwrap();
        let mut scene = plico_renderer::Scene::new();
        s.build_and_paint(&mut scene, &mut tcx, 400, 300, 1.);
        snapshots.push((
            s.adapter.stats.paint_writes,
            s.adapter.stats.style_writes,
            s.adapter.stats.flatten_visits,
            s.adapter.stats.scene_builds,
            s.adapter.stats.scene_reuses,
        ));
    }
    // Frame 0 does the work; frames 1..3 must be fully retained.
    for (i, stats) in snapshots.iter().enumerate().skip(1) {
        assert_eq!(stats.0, 0, "tick {i}: paint churn {stats:?}");
        assert_eq!(stats.1, 0, "tick {i}: style churn {stats:?}");
    }
}
