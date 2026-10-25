//! Single-threaded benchmark allocation counts; disabled during normal examples/tests.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
struct Counted;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: Counted = Counted;
// SAFETY: allocations are delegated unchanged to System; counters only observe.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Relaxed) {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Relaxed) {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Relaxed) {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size, Relaxed);
        }
        unsafe { System.realloc(ptr, old, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
pub(super) fn start() {
    CALLS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ACTIVE.store(true, Relaxed);
}
pub(super) fn stop() -> (usize, usize) {
    ACTIVE.store(false, Relaxed);
    (CALLS.load(Relaxed), BYTES.load(Relaxed))
}
