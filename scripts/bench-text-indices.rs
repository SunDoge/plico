//! Standalone allocation probe; measures the index helper, not native text or UI.
//! rustc -O --edition=2024 scripts/bench-text-indices.rs -o /tmp/plico-text-indices-bench
//! /tmp/plico-text-indices-bench
#[path = "../crates/plico-native/src/text_indices.rs"]
mod text_indices;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Counted;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
// SAFETY: Every request is forwarded unchanged to System; counters never allocate.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: Forwarding the caller's valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Forwarding the matching pointer/layout to its allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size, Ordering::Relaxed);
        // SAFETY: Forwarding the caller's valid reallocation request unchanged.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;
fn main() {
    for (name, text) in [
        ("ascii", "entry".repeat(64)),
        ("unicode", "中文🙂".repeat(32)),
        ("mixed", format!("{}🙂", "entry".repeat(64))),
    ] {
        let units = text.encode_utf16().count() as u32;
        ALLOCATIONS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        let start = Instant::now();
        for _ in 0..10_000 {
            let indices = text_indices::TextIndices::new(black_box(&text));
            black_box(indices.utf16(text.len()));
            black_box(indices.byte(units));
            black_box(indices);
        }
        let elapsed = start.elapsed();
        let allocations = ALLOCATIONS.load(Ordering::Relaxed);
        let bytes = BYTES.load(Ordering::Relaxed);
        println!(
            "{name}: iterations=10000 allocations={allocations} requested_bytes={bytes} elapsed_us={}",
            elapsed.as_micros()
        );
    }
}
