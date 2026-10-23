//! Scene-encoding cost scaling: one paint write at N placed text nodes.
//! CPU only; no GPU or present. Complements memo_backend_probe, which
//! reports whole-frame totals.
use plico_backend::{Source, View};
use plico_core::{Color, UiContext, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::Duration;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
struct Counted;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(l.size(), Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, s: usize) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(s, Relaxed);
        }
        unsafe { System.realloc(p, l, s) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        unsafe { self.alloc(l) }
    }
}
#[global_allocator]
static A: Counted = Counted;
struct Demo {
    count: usize,
    changed: bool,
}
impl View for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        cx.column().children(|cx| {
            for i in 0..self.count {
                cx.text("Retained paragraph")
                    .key(i as u64)
                    .color(if i == 0 && self.changed {
                        Color::Literal(0xff0000)
                    } else {
                        Color::Literal(0x222222)
                    });
            }
        });
    }
}
fn main() {
    println!(
        "nodes,paint_writes,scene_p50_us,scene_p95_us,build_p50_us,scene_builds,scene_reuses,allocs_per_changed_frame,bytes_per_changed_frame"
    );
    for count in [100, 500, 1_000, 2_000, 4_000] {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(
            Demo {
                count,
                changed: false,
            },
            clock.clone(),
        );
        let mut text = TextContext::new();
        let mut scene = Scene::new();
        // Warmup: first declaration, layout, text shaping, scene build.
        source.build_and_paint(&mut scene, &mut text, 800, 600, 1.);
        let mut scene_us = Vec::new();
        let mut build_us = Vec::new();
        let mut builds = 0;
        let mut reuses = 0;
        let mut allocs = 0;
        let mut bytes = 0;
        for frame in 0..120 {
            clock.advance(Duration::from_millis(16)).unwrap();
            source.view.changed = frame % 2 == 0;
            source.request_rebuild();
            ALLOCS.store(0, Relaxed);
            BYTES.store(0, Relaxed);
            ENABLED.store(true, Relaxed);
            let (_, build_ms, scene_ms) =
                source.build_and_paint(&mut scene, &mut text, 800, 600, 1.);
            ENABLED.store(false, Relaxed);
            assert!(source.last_error.is_none(), "{:?}", source.last_error);
            // Only sample the changed frames; odd frames declare unchanged
            // and take the retained path.
            if frame >= 16 && source.view.changed {
                scene_us.push(scene_ms * 1e3);
                build_us.push(build_ms * 1e3);
                builds += source.adapter.stats.scene_builds;
                reuses += source.adapter.stats.scene_reuses;
                allocs = ALLOCS.load(Relaxed);
                bytes = BYTES.load(Relaxed);
            }
        }
        scene_us.sort_by(f64::total_cmp);
        build_us.sort_by(f64::total_cmp);
        let writes = source.adapter.stats.paint_writes;
        println!(
            "{count},{writes},{:.3},{:.3},{:.3},{builds},{reuses},{allocs},{bytes}",
            scene_us[26], scene_us[49], build_us[26],
        );
    }
}
