//! Reproducible CPU/allocation probe. Run with --release; no window or GPU.
use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
struct CountingAllocator;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size, Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
#[derive(Default)]
struct Demo {
    extra: bool,
}
impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        let _ = column(cx).children(|cx| {
            for i in 0..1000 {
                text(cx, "Static row").key(i as u64);
            }
            let _ = column(cx).key("changing").children(|cx| {
                if self.extra {
                    text(cx, "Added row").key("extra");
                }
            });
        });
    }
}
fn main() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
        .unwrap();
    for structural in [false, true] {
        let mut samples = Vec::with_capacity(200);
        let mut allocations = 0;
        let mut bytes = 0;
        let mut visits = 0;
        let mut declaration_us = 0;
        let mut sync_us = 0;
        let mut layout_us = 0;
        let mut scene_us = 0.;
        for frame in 0..220 {
            source.view.extra = structural && frame % 2 == 0;
            source.request_rebuild();
            ALLOCS.store(0, Relaxed);
            BYTES.store(0, Relaxed);
            ACTIVE.store(true, Relaxed);
            let start = Instant::now();
            let (_, _, scene_ms) = source
                .render_retained_frame(&mut scene, &mut text, (800, 600), 1.)
                .unwrap();
            let elapsed = start.elapsed().as_nanos();
            ACTIVE.store(false, Relaxed);
            assert!(source.last_error.is_none());
            assert_eq!(source.adapter.stats.placed_node_clones, 0);
            if frame >= 20 {
                samples.push(elapsed);
                allocations += ALLOCS.load(Relaxed);
                bytes += BYTES.load(Relaxed);
                visits += source.adapter.stats.sync_visits;
                declaration_us += source.declaration_micros;
                sync_us += source.adapter.stats.sync_micros;
                layout_us += source.adapter.stats.layout_micros;
                scene_us += scene_ms * 1000.;
            }
        }
        samples.sort_unstable();
        println!(
            "{}: p50={}us p95={}us p99={}us allocs/frame={} allocated-bytes/frame={} sync-visits/frame={:.1}",
            if structural {
                "insert/remove"
            } else {
                "unchanged declaration"
            },
            samples[100] / 1000,
            samples[190] / 1000,
            samples[198] / 1000,
            allocations / 200,
            bytes / 200,
            visits as f64 / 200.
        );
        println!(
            "  mean phases: declaration={}us sync={}us layout+flatten={}us scene={:.1}us",
            declaration_us / 200,
            sync_us / 200,
            layout_us / 200,
            scene_us / 200.
        );
    }
}
