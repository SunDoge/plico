//! Isolated, single-threaded allocator probe. Unsafe code is confined to this probe.
use plico::{Color, Engine, UiContext};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
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
fn view(ui: &mut UiContext<'_>, count: usize, color: Color, edit: bool) {
    for index in 0..count {
        let _ = ui.column().key(index).width(120.).children(|ui| {
            ui.text(if edit && index % 1000 == 0 {
                "edited"
            } else {
                "stable"
            })
            .color(color);
        });
    }
}
fn measured(
    engine: &mut Engine,
    count: usize,
    color: Color,
    edit: bool,
) -> (usize, usize, plico::FrameStats) {
    measured_view(engine, |ui| view(ui, count, color, edit))
}
fn measured_view(
    engine: &mut Engine,
    view: impl FnOnce(&mut UiContext<'_>),
) -> (usize, usize, plico::FrameStats) {
    CALLS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    let result = engine.frame(view);
    ENABLED.store(false, Relaxed);
    (CALLS.load(Relaxed), BYTES.load(Relaxed), result.unwrap())
}
fn main() {
    let count = 10_000;
    let mut engine = Engine::default();
    let cold = measured(&mut engine, count, Color::Literal(0x222222), false);
    let second = measured(&mut engine, count, Color::Literal(0x222222), false);
    assert_eq!((second.0, second.1), (0, 0));
    let idle = measured(&mut engine, count, Color::Literal(0x222222), false);
    let color = measured(&mut engine, count, Color::Literal(0x333333), false);
    let text = measured(&mut engine, count, Color::Literal(0x333333), true);
    println!(
        "cold: {cold:?}\nsecond: {second:?}\nunchanged: {idle:?}\ncolor: {color:?}\ntext (same capacity): {text:?}"
    );
    assert_eq!(
        (idle.0, idle.1, idle.2.measure_calls, idle.2.layout_runs),
        (0, 0, 0, 0)
    );
    assert_eq!(
        (color.0, color.1, color.2.measure_calls, color.2.layout_runs),
        (0, 0, 0, 0)
    );
    assert_eq!((text.0, text.1, text.2.measure_calls), (0, 0, 10));
    // Isolate temporary format! allocations from retained text writes.
    let labels: Vec<_> = (0..count).map(|i| format!("Value: {i}")).collect();
    let mut strings = Engine::default();
    let formatted = |ui: &mut UiContext<'_>| {
        for i in 0..count {
            ui.text(format!("Value: {i}")).key(i);
        }
    };
    measured_view(&mut strings, formatted);
    measured_view(&mut strings, formatted);
    let format_each_frame = measured_view(&mut strings, formatted);
    let borrowed = measured_view(&mut strings, |ui| {
        for (i, label) in labels.iter().enumerate() {
            ui.text(label.as_str()).key(i);
        }
    });
    assert!(format_each_frame.0 >= count);
    assert_eq!(format_each_frame.2.text_writes, 0);
    assert_eq!((borrowed.0, borrowed.1, borrowed.2.text_writes), (0, 0, 0));
    println!("unchanged format!: {format_each_frame:?}\nunchanged borrowed labels: {borrowed:?}");
    // Reuse both the formatter scratch and each retained node's text capacity.
    fn reusable(ui: &mut UiContext<'_>, count: usize, revision: usize) {
        for i in 0..count {
            plico::text!(ui, "Value: {:05}", i + revision).key(i);
        }
    }
    let mut buffered = Engine::default();
    measured_view(&mut buffered, |ui| reusable(ui, count, 0));
    measured_view(&mut buffered, |ui| reusable(ui, count, 0));
    let macro_unchanged = measured_view(&mut buffered, |ui| reusable(ui, count, 0));
    measured_view(&mut buffered, |ui| reusable(ui, count, 10_000));
    let macro_changed = measured_view(&mut buffered, |ui| reusable(ui, count, 20_000));
    assert_eq!(
        (
            macro_unchanged.0,
            macro_unchanged.1,
            macro_unchanged.2.text_writes
        ),
        (0, 0, 0)
    );
    assert_eq!(
        (
            macro_changed.0,
            macro_changed.1,
            macro_changed.2.text_writes
        ),
        (0, 0, count)
    );
    println!(
        "unchanged text!: {macro_unchanged:?}\nchanged text! (same capacity): {macro_changed:?}"
    );
}
