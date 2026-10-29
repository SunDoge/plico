//! Isolated full-tree flatten CPU/allocation probe; no text shaping, Scene or GPU.
//! Run with `cargo run --release -p plico-renderer --example flatten_probe`.
use plico_renderer::{
    layout::flatten_with_scroll_into,
    style::{Paint, PaintTransform},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::HashMap,
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    },
    time::Instant,
};
use taffy::{AvailableSpace, Dimension, Size, TaffyTree};
struct Allocator;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Allocator {
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
static ALLOCATOR: Allocator = Allocator;
fn measure(label: &str, nodes: usize, mut run: impl FnMut()) {
    const ITERATIONS: usize = 200;
    for _ in 0..50 {
        run();
    }
    let mut samples = Vec::with_capacity(ITERATIONS);
    let (mut allocations, mut bytes) = (0, 0);
    for _ in 0..ITERATIONS {
        ALLOCS.store(0, Relaxed);
        BYTES.store(0, Relaxed);
        ACTIVE.store(true, Relaxed);
        let start = Instant::now();
        run();
        let elapsed = start.elapsed().as_nanos();
        ACTIVE.store(false, Relaxed);
        allocations += ALLOCS.load(Relaxed);
        bytes += BYTES.load(Relaxed);
        samples.push(elapsed);
    }
    samples.sort_unstable();
    println!(
        "{label},{nodes},{},{},{},{}",
        samples[ITERATIONS / 2],
        samples[ITERATIONS * 95 / 100],
        allocations / ITERATIONS,
        bytes / ITERATIONS
    );
}
fn main() {
    println!("case,nodes,p50_ns,p95_ns,allocations,requested_bytes");
    for leaves in [1000, 10000] {
        for rich in [false, true] {
            let mut tree = TaffyTree::new();
            let mut ids = Vec::new();
            let mut rows = Vec::new();
            for _ in 0..leaves / 20 {
                let mut children = Vec::new();
                for index in 0..20 {
                    let paint = Paint {
                        text: Some(Arc::from("A retained text label")),
                        transform: if rich {
                            vec![PaintTransform::Scale(1., 1.)]
                        } else {
                            Vec::new()
                        },
                        z_index: if rich { 20 - index } else { 0 },
                        ..Paint::default()
                    };
                    let id = tree
                        .new_leaf_with_context(
                            taffy::Style {
                                size: Size {
                                    width: Dimension::length(100.),
                                    height: Dimension::length(24.),
                                },
                                flex_shrink: 0.,
                                ..Default::default()
                            },
                            paint,
                        )
                        .unwrap();
                    ids.push(id);
                    children.push(id);
                }
                let row = tree
                    .new_with_children(
                        taffy::Style {
                            overflow: taffy::Point {
                                x: taffy::Overflow::Hidden,
                                y: taffy::Overflow::Hidden,
                            },
                            size: Size {
                                width: Dimension::length(1000.),
                                height: Dimension::length(24.),
                            },
                            flex_shrink: 0.,
                            ..Default::default()
                        },
                        &children,
                    )
                    .unwrap();
                tree.set_node_context(row, Some(Paint::default())).unwrap();
                ids.push(row);
                rows.push(row);
            }
            let root = tree
                .new_with_children(
                    taffy::Style {
                        flex_direction: taffy::FlexDirection::Column,
                        size: Size {
                            width: Dimension::length(1000.),
                            height: Dimension::length((leaves / 20 * 24) as f32),
                        },
                        ..Default::default()
                    },
                    &rows,
                )
                .unwrap();
            tree.set_node_context(root, Some(Paint::default())).unwrap();
            ids.push(root);
            tree.compute_layout(
                root,
                Size {
                    width: AvailableSpace::MaxContent,
                    height: AvailableSpace::MaxContent,
                },
            )
            .unwrap();
            let offsets = HashMap::new();
            let mut out = Vec::new();
            let label = if rich {
                "transformed_unsorted"
            } else {
                "plain_sorted"
            };
            measure(label, ids.len(), || {
                flatten_with_scroll_into(black_box(&tree), root, &offsets, &mut out);
                assert_eq!(out.len(), ids.len());
                black_box(&out);
            });
            let mut paints = Vec::with_capacity(ids.len());
            measure(&format!("{label}_paint_clone"), ids.len(), || {
                paints.clear();
                for id in &ids {
                    paints.push(black_box(tree.get_node_context(*id).unwrap()).clone());
                }
                black_box(&paints);
            });
        }
    }
}
