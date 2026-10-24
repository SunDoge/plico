//! Core-only update-cost experiment, not a memo API or renderer benchmark.
//! Run: cargo run --release -p plico --example memo_probe
use plico_core::{Engine, UiContext};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
use std::{hint::black_box, time::Instant};
struct Counted;
static ENABLED: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
// SAFETY: every allocation is delegated unchanged to System; atomics only observe it.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size, Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;

const WARMUP: usize = 16;
const SAMPLES: usize = 120;
#[derive(Clone, Copy)]
enum Labels {
    Borrowed,
    Format,
    Buffered,
}
struct Fixture {
    labels: Vec<String>,
}
impl Fixture {
    fn new(rows: usize) -> Self {
        Self {
            labels: (0..rows)
                .map(|i| format!("Node {i:05} · Tokyo · available"))
                .collect(),
        }
    }
    fn show(&self, ui: &mut UiContext<'_>, labels: Labels, changed: bool) {
        for (i, label) in self.labels.iter().enumerate() {
            ui.row().key(i).gap(8.).padding(4.).children(|ui| {
                match labels {
                    Labels::Borrowed => {
                        ui.text(label.as_str());
                    }
                    Labels::Format => {
                        ui.text(format!("Node {i:05} · Tokyo · available"));
                    }
                    Labels::Buffered => {
                        plico_core::text!(ui, "Node {i:05} · Tokyo · available");
                    }
                }
                ui.text(if changed && i == 0 { "Busy" } else { "Idle" });
            });
        }
    }
}
struct Sample {
    total: f64,
    declaration: f64,
    calls: usize,
    bytes: usize,
    visited: usize,
    writes: usize,
}
fn sample(engine: &mut Engine, fixture: &Fixture, labels: Labels, changed: bool) -> Sample {
    CALLS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    let mut declaration = 0.;
    let start = Instant::now();
    ENABLED.store(true, Relaxed);
    let stats = engine
        .frame_declarations(|ui| {
            let start = Instant::now();
            fixture.show(ui, labels, changed);
            declaration = start.elapsed().as_secs_f64() * 1e6;
        })
        .unwrap();
    ENABLED.store(false, Relaxed);
    let total = start.elapsed().as_secs_f64() * 1e6;
    // This experiment deliberately excludes measurement/layout, GPU and FFI.
    assert_eq!(
        (
            stats.created,
            stats.removed,
            stats.measure_calls,
            stats.layout_runs
        ),
        (0, 0, 0, 0)
    );
    Sample {
        total,
        declaration,
        calls: CALLS.load(Relaxed),
        bytes: BYTES.load(Relaxed),
        visited: stats.visited,
        writes: stats.text_writes,
    }
}
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}
fn report(name: &str, rows: usize, samples: &[Sample]) {
    let n = samples.len() as f64;
    let mut totals: Vec<_> = samples.iter().map(|s| s.total).collect();
    let mut declarations: Vec<_> = samples.iter().map(|s| s.declaration).collect();
    let mut commits: Vec<_> = samples.iter().map(|s| s.total - s.declaration).collect();
    println!(
        "{name},{rows},{:.3},{:.3},{:.3},{:.3},{:.1},{:.1},{:.1},{:.1}",
        percentile(&mut totals, 0.5),
        percentile(&mut totals, 0.95),
        percentile(&mut declarations, 0.5),
        percentile(&mut commits, 0.5),
        samples.iter().map(|s| s.calls).sum::<usize>() as f64 / n,
        samples.iter().map(|s| s.bytes).sum::<usize>() as f64 / n,
        samples.iter().map(|s| s.visited).sum::<usize>() as f64 / n,
        samples.iter().map(|s| s.writes).sum::<usize>() as f64 / n
    );
}
fn warm(engine: &mut Engine, fixture: &Fixture, labels: Labels) {
    for _ in 0..WARMUP {
        black_box(
            engine
                .frame_declarations(|ui| fixture.show(ui, labels, false))
                .unwrap(),
        );
    }
}
fn main() {
    println!(
        "case,rows,total_p50_us,total_p95_us,declaration_p50_us,commit_p50_us,alloc_calls,alloc_bytes,visited,text_writes"
    );
    for rows in [100, 1_000, 10_000] {
        let fixture = Fixture::new(rows);
        for (name, labels, sparse) in [
            ("stable_borrowed", Labels::Borrowed, false),
            ("sparse_borrowed", Labels::Borrowed, true),
            ("stable_format", Labels::Format, false),
            ("stable_text_macro", Labels::Buffered, false),
        ] {
            let mut engine = Engine::default();
            warm(&mut engine, &fixture, labels);
            let samples: Vec<_> = (0..SAMPLES)
                .map(|i| sample(&mut engine, &fixture, labels, sparse && i % 2 == 0))
                .collect();
            assert!(samples.iter().all(|s| s.visited == rows * 3));
            if !sparse {
                assert!(samples.iter().all(|s| s.writes == 0));
            }
            report(name, rows, &samples);
        }
    }
    // Architectural lower bound only: 100 separate retained trees; leave 99
    // untouched. No shared layout, events, theme propagation or scene composition.
    let fixture = Fixture::new(100);
    let mut islands: Vec<_> = (0..100)
        .map(|_| {
            let mut e = Engine::default();
            warm(&mut e, &fixture, Labels::Borrowed);
            e
        })
        .collect();
    let samples: Vec<_> = (0..SAMPLES)
        .map(|i| sample(&mut islands[0], &fixture, Labels::Borrowed, i % 2 == 0))
        .collect();
    report("isolated_1_of_100_NOT_memo", 10_000, &samples);
    black_box(islands);
}
