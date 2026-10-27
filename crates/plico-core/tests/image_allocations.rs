//! Guard the steady-state declaration cost independently of GPU and timing noise.
use plico_core::{Engine, ImageAsset, widgets::image};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

struct CountingAllocator;
thread_local! {
    static ALLOCATED: Cell<Option<usize>> = const { Cell::new(None) };
}
fn record(bytes: usize) {
    let _ = ALLOCATED.try_with(|count| {
        if let Some(total) = count.get() {
            count.set(Some(total.saturating_add(bytes)));
        }
    });
}
// SAFETY: all operations delegate unchanged pointers/layouts to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn repeated_image_declarations_do_not_allocate_image_sized_strings() {
    // The core transports bytes; decoding is deliberately outside this test.
    let asset = ImageAsset::from_bytes(&vec![42; 256 * 1024]);
    let mut engine = Engine::default();
    let mut frame = || {
        engine
            .frame_declarations(|cx| {
                image(cx, &asset).size(64., 64.);
            })
            .unwrap();
    };
    for _ in 0..3 {
        frame();
    }
    ALLOCATED.with(|count| count.set(Some(0)));
    for _ in 0..5 {
        frame();
    }
    let allocated = ALLOCATED.with(|count| count.replace(None).unwrap());
    assert!(
        allocated < 64 * 1024,
        "five warm frames allocated {allocated} bytes for an unchanged image"
    );
}
