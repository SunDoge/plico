//! Allocation comparison for prepared command traversal, not a GUI/frame benchmark.
use plico_renderer::{PaintScene, Scene};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    time::Instant,
};
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}
struct Counted;
fn record(bytes: usize) {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCS.try_with(|count| count.set(count.get() + 1));
        let _ = BYTES.try_with(|count| count.set(count.get() + bytes));
    }
}
// SAFETY: Layouts and pointers are passed unchanged to System; TLS counters do not allocate.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: Forwarding the valid caller-provided layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Forwarding the matching pointer and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: Forwarding the valid caller-provided reallocation request.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;
fn main() -> plico_wgpu::Result<()> {
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new())?;
    let mut scene = Scene::new();
    for i in 0..10_000 {
        let x = f64::from(i % 100);
        let y = f64::from(i / 100);
        scene.fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &kurbo::Rect::new(x, y, x + 1., y + 1.),
        );
    }
    let prepared = renderer.prepare_scene(&scene, [128, 128])?;
    for borrowed in [false, true] {
        ALLOCS.set(0);
        BYTES.set(0);
        TRACK.set(true);
        let started = Instant::now();
        let mut count = 0;
        for _ in 0..100 {
            if borrowed {
                for draw in prepared.draw_iter() {
                    black_box(draw);
                    count += 1;
                }
            } else {
                for draw in prepared.draws() {
                    black_box(draw);
                    count += 1;
                }
            }
        }
        let elapsed = started.elapsed();
        TRACK.set(false);
        let allocations = ALLOCS.get();
        let bytes = BYTES.get();
        assert_eq!(count, prepared.draw_iter().len() * 100);
        if borrowed {
            assert_eq!(allocations, 0);
        }
        println!(
            "prepared-traversal: borrowed={borrowed} draws={count} allocations={allocations} requested_bytes={bytes} elapsed_us={}",
            elapsed.as_micros()
        );
    }
    ALLOCS.set(0);
    BYTES.set(0);
    TRACK.set(true);
    let started = Instant::now();
    for _ in 0..100 {
        black_box(renderer.prepare_scene(&scene, [128, 128])?);
    }
    let elapsed = started.elapsed();
    TRACK.set(false);
    let allocations = ALLOCS.get();
    let bytes = BYTES.get();
    assert_eq!(allocations, 0, "warm prepared-scene hits allocated");
    println!(
        "prepared-cache-hit: calls=100 allocations={allocations} requested_bytes={bytes} elapsed_us={}",
        elapsed.as_micros()
    );
    Ok(())
}
