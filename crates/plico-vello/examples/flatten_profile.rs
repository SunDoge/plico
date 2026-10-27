//! CPU-only retained-frame profiling, with precise layout/flatten spans.
//! cargo run --release -p plico-vello --features profiling --example flatten_profile
//! Clone timings are a separate warm-buffer lower-bound comparison, not a
//! subtraction-based estimate of traversal cost. No GPU/present is measured.
use plico_core::{VirtualClock, prelude::*};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, Scene, TextContext, UiEvent, WheelDeltaMode,
    WheelEvent,
};
use plico_vello::{Source, View};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

#[derive(Default, Clone, Copy)]
struct Counters {
    layout_ns: u128,
    flatten_ns: u128,
    flatten_calls: usize,
    selection_ns: u128,
    presentation_ns: u128,
    index_ns: u128,
}
struct Timer(Instant);
struct Collector(Arc<Mutex<Counters>>);
impl<S: tracing::Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Collector {
    fn on_enter(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        ctx.span(id)
            .unwrap()
            .extensions_mut()
            .insert(Timer(Instant::now()));
    }
    fn on_exit(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let elapsed = span
            .extensions()
            .get::<Timer>()
            .unwrap()
            .0
            .elapsed()
            .as_nanos();
        let mut counters = self.0.lock().unwrap();
        match span.name() {
            "layout.compute" => counters.layout_ns += elapsed,
            "backend.selection" => counters.selection_ns += elapsed,
            "backend.presentation" => counters.presentation_ns += elapsed,
            "backend.index" => counters.index_ns += elapsed,
            "layout.flatten" => {
                counters.flatten_ns += elapsed;
                counters.flatten_calls += 1;
            }
            _ => {}
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Case {
    Idle,
    Redeclare,
    Color,
    Opacity,
    Translate,
    Size,
    Scroll,
    VirtualScroll,
    Structure,
}
struct App {
    rows: usize,
    case: Case,
    flip: bool,
}
impl View for App {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        if matches!(self.case, Case::VirtualScroll) {
            virtual_list(cx, self.rows, 24.).width(780.).height(560.).smooth_scroll(false).rows(
                |i| i,
                |cx, _| {
                    text(cx, "Retained row");
                },
            );
            return;
        }
        let alpha = if self.flip && matches!(self.case, Case::Opacity) { 0.8 } else { 1. };
        column(cx).width(780.).height(560.).scroll_y(true).smooth_scroll(false).opacity(alpha) + |cx| {
            for index in 0..self.rows {
                let first = index == 0;
                let fill = if first && self.flip && matches!(self.case, Case::Color) {
                    Color::Accent
                } else {
                    Color::Panel
                };
                let width = if first && self.flip && matches!(self.case, Case::Size) { 779. } else { 780. };
                let x = if first && self.flip && matches!(self.case, Case::Translate) { 3. } else { 0. };
                row(cx).key(index).width(width).height(24.).shrink(0.).background(fill).translate(x, 0.) + |cx| {
                    text(cx, "Retained row");
                };
            }
            if self.flip && matches!(self.case, Case::Structure) {
                text(cx, "Conditional row");
            }
        };
    }
}
fn percentile(samples: &mut [f64], q: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * q).round() as usize]
}
fn clone_cost(nodes: &[plico_renderer::layout::PlacedNode]) -> f64 {
    let mut copies = Vec::with_capacity(nodes.len());
    let mut clone_us = Vec::new();
    for i in 0..140 {
        let start = Instant::now();
        copies.clear();
        copies.extend_from_slice(nodes);
        std::hint::black_box(&copies);
        if i >= 20 {
            clone_us.push(start.elapsed().as_secs_f64() * 1e6);
        }
    }
    percentile(&mut clone_us, 0.5)
}

fn run(rows: usize, case: Case, counters: &Arc<Mutex<Counters>>) {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        App {
            rows,
            case,
            flip: false,
        },
        clock.clone(),
    );
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
        .unwrap();
    let (mut total, mut layout, mut flatten) = (Vec::new(), Vec::new(), Vec::new());
    let (mut sum_total, mut sum_flatten, mut calls, mut visits) = (0., 0., 0, 0);
    let mut phases = [0.; 7];
    for i in 0..160 {
        clock.advance(Duration::from_millis(16)).unwrap();
        source.view.flip = i % 2 == 0;
        if matches!(case, Case::Scroll | Case::VirtualScroll) {
            assert!(
                source
                    .handle_event(UiEvent::Wheel(WheelEvent {
                        position: Point { x: 20., y: 20. },
                        delta_x: 0.,
                        delta_y: if i % 2 == 0 { 24. } else { -24. },
                        delta_mode: WheelDeltaMode::Pixel,
                        phase: GesturePhase::Changed,
                        modifiers: Modifiers::empty(),
                    }))
                    .handled
            );
        } else if !matches!(case, Case::Idle) {
            source.request_rebuild();
        }
        *counters.lock().unwrap() = Counters::default();
        let start = Instant::now();
        let (_, _, scene_ms) = source
            .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
            .unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1e6;
        assert!(source.last_error.is_none(), "{:?}", source.last_error);
        if i >= 40 {
            let c = *counters.lock().unwrap();
            total.push(elapsed);
            layout.push(c.layout_ns as f64 / 1000.);
            flatten.push(c.flatten_ns as f64 / 1000.);
            sum_total += elapsed;
            sum_flatten += c.flatten_ns as f64 / 1000.;
            calls += c.flatten_calls;
            visits += source.adapter.stats.flatten_visits;
            phases[0] += source.declaration_micros as f64;
            phases[1] += source.adapter.stats.sync_micros as f64;
            phases[2] += source.adapter.stats.layout_micros as f64;
            phases[3] += scene_ms * 1000.;
            phases[4] += c.selection_ns as f64 / 1000.;
            phases[5] += c.presentation_ns as f64 / 1000.;
            phases[6] += c.index_ns as f64 / 1000.;
        }
    }
    println!(
        "{rows},{case:?},{},{:.3},{:.3},{:.3},{:.3},{:.2},{:.2},{:.0},{:.3},{}",
        source.adapter.placed.len(),
        percentile(&mut total, 0.5),
        percentile(&mut total, 0.95),
        percentile(&mut layout, 0.5),
        percentile(&mut flatten, 0.5),
        sum_flatten / sum_total * 100.,
        calls as f64 / 120.,
        visits as f64 / 120.,
        clone_cost(&source.adapter.placed),
        phases.map(|v| format!("{:.3}", v / 120.)).join(",")
    );
}

fn run_gallery(page: gallery::Page, label: &str, resize: bool, counters: &Arc<Mutex<Counters>>) {
    let clock = VirtualClock::default();
    let mut app = gallery::Gallery::default();
    app.router.go(page);
    let mut source = Source::with_clock(plico_vello::NativeApp(app), clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    let (mut totals, mut layouts, mut flattens) = (Vec::new(), Vec::new(), Vec::new());
    let (mut sum_total, mut sum_flatten, mut calls, mut visits) = (0., 0., 0, 0);
    let mut phases = [0.; 7];
    for frame in 0..180 {
        clock.advance(Duration::from_millis(16)).unwrap();
        if matches!(page, gallery::Page::Motion) && frame % 60 == 0 {
            source.view.0.expanded = !source.view.0.expanded;
            source.request_rebuild();
        }
        let width = 1280 + u32::from(resize && frame % 2 == 0);
        *counters.lock().unwrap() = Counters::default();
        let start = Instant::now();
        let (_, _, scene_ms) = source
            .render_retained_frame(&mut scene, &mut text, (width, 850), 1.)
            .unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1e6;
        assert!(source.last_error.is_none(), "{:?}", source.last_error);
        if frame >= 60 {
            let c = *counters.lock().unwrap();
            totals.push(elapsed);
            layouts.push(c.layout_ns as f64 / 1000.);
            flattens.push(c.flatten_ns as f64 / 1000.);
            sum_total += elapsed;
            sum_flatten += c.flatten_ns as f64 / 1000.;
            calls += c.flatten_calls;
            visits += source.adapter.stats.flatten_visits;
            phases[0] += source.declaration_micros as f64;
            phases[1] += source.adapter.stats.sync_micros as f64;
            phases[2] += source.adapter.stats.layout_micros as f64;
            phases[3] += scene_ms * 1000.;
            phases[4] += c.selection_ns as f64 / 1000.;
            phases[5] += c.presentation_ns as f64 / 1000.;
            phases[6] += c.index_ns as f64 / 1000.;
        }
    }
    println!(
        "0,{label},{},{:.3},{:.3},{:.3},{:.3},{:.2},{:.2},{:.0},{:.3},{}",
        source.adapter.placed.len(),
        percentile(&mut totals, 0.5),
        percentile(&mut totals, 0.95),
        percentile(&mut layouts, 0.5),
        percentile(&mut flattens, 0.5),
        sum_flatten / sum_total * 100.,
        calls as f64 / 120.,
        visits as f64 / 120.,
        clone_cost(&source.adapter.placed),
        phases.map(|v| format!("{:.3}", v / 120.)).join(",")
    );
}

fn main() {
    let counters = Arc::new(Mutex::new(Counters::default()));
    use tracing_subscriber::prelude::*;
    let subscriber = tracing_subscriber::registry().with(Collector(counters.clone()).with_filter(
        tracing_subscriber::filter::filter_fn(|m| m.target() == "plico::layout"),
    ));
    tracing::subscriber::with_default(subscriber, || {
        println!(
            "rows,case,placed,frame_p50_us,frame_p95_us,taffy_p50_us,flatten_p50_us,flatten_mean_share_pct,flatten_calls_per_frame,flatten_visits_per_frame,clone_p50_us,declaration_mean_us,sync_mean_us,backend_mean_us,scene_mean_us,selection_mean_us,presentation_mean_us,index_mean_us"
        );
        for (page, label, resize) in [
            (gallery::Page::Workspace, "GalleryWorkspaceIdle", false),
            (gallery::Page::Controls, "GalleryControlsIdle", false),
            (gallery::Page::Layout, "GalleryLayoutResize", true),
            (gallery::Page::Motion, "GalleryMotion", false),
        ] {
            run_gallery(page, label, resize, &counters);
        }
        for rows in [200, 2000, 10000] {
            for case in [
                Case::Idle,
                Case::Redeclare,
                Case::Color,
                Case::Opacity,
                Case::Translate,
                Case::Size,
                Case::Scroll,
                Case::VirtualScroll,
                Case::Structure,
            ] {
                run(rows, case, &counters);
            }
        }
    });
}
