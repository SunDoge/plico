//! CPU-only, shared-tree experiment. See README.md for scope and limitations.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::RefCell,
    hint::black_box,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
struct CountingAllocator;
static COUNT: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
// SAFETY: all memory operations are forwarded unchanged to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNT.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size, Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
use kurbo::{Affine, Rect, Shape};
use peniko::{Color, Fill};
use plico_renderer::{PaintCommand, PaintScene, Scene};
use sycamore_reactive::{
    Signal, batch, create_child_scope, create_effect, create_root, create_signal,
};
use taffy::{TaffyTree, prelude::*};

const WARMUP: usize = 12;
const SAMPLES: usize = 60;
#[derive(Clone, Copy, Debug)]
enum Driver {
    Naive,
    Full,
    Dirty,
    Reactive,
}
impl Driver {
    fn name(self) -> &'static str {
        match self {
            Self::Naive => "naive",
            Self::Full => "full",
            Self::Dirty => "dirty",
            Self::Reactive => "reactive",
        }
    }
}
#[derive(Clone, Copy)]
enum Work {
    Paint,
    Layout,
}
impl Work {
    fn name(self) -> &'static str {
        match self {
            Self::Paint => "paint",
            Self::Layout => "layout",
        }
    }
}
fn style(value: u8) -> Style {
    Style {
        size: Size {
            width: Dimension::length(40. + f32::from(value)),
            height: Dimension::length(12.),
        },
        ..Default::default()
    }
}
struct Shared {
    tree: TaffyTree<()>,
    root: NodeId,
    nodes: Vec<NodeId>,
    values: Vec<u8>,
    scene: Scene,
}
impl Shared {
    fn new(n: usize) -> Self {
        let mut tree = TaffyTree::new();
        let nodes: Vec<_> = (0..n).map(|_| tree.new_leaf(style(0)).unwrap()).collect();
        let root = tree
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                &nodes,
            )
            .unwrap();
        tree.compute_layout(root, Size::MAX_CONTENT).unwrap();
        let mut shared = Self {
            tree,
            root,
            nodes,
            values: vec![0; n],
            scene: Scene::new(),
        };
        shared.paint_full();
        shared
    }
    fn apply(&mut self, updates: &[(usize, u8)], work: Work) {
        for &(id, value) in updates {
            assert_ne!(self.values[id], value, "duplicate or stale update");
            self.values[id] = value;
            if matches!(work, Work::Layout) {
                self.tree.set_style(self.nodes[id], style(value)).unwrap();
            }
        }
    }
    fn layout(&mut self, work: Work) {
        if matches!(work, Work::Layout) {
            self.tree
                .compute_layout(self.root, Size::MAX_CONTENT)
                .unwrap();
        }
    }
    fn paint_full(&mut self) {
        self.scene.reset();
        for (i, &node) in self.nodes.iter().enumerate() {
            let layout = self.tree.layout(node).unwrap();
            let rect = Rect::new(
                layout.location.x as f64,
                layout.location.y as f64,
                (layout.location.x + layout.size.width) as f64,
                (layout.location.y + layout.size.height) as f64,
            );
            self.scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                if self.values[i] == 0 {
                    Color::BLACK
                } else {
                    Color::WHITE
                },
                None,
                &rect,
            );
        }
        black_box(&self.scene);
    }
    // Ideal retained display-list model: fixed-height rows at fixed positions.
    // It is deliberately not presented as Plico's current renderer behavior.
    fn paint_retained(&mut self, updates: &[(usize, u8)], work: Work) {
        for &(i, value) in updates {
            let PaintCommand::Fill { brush, shape, .. } = &mut self.scene.commands[i] else {
                unreachable!()
            };
            *brush = if value == 0 {
                Color::BLACK
            } else {
                Color::WHITE
            }
            .into();
            if matches!(work, Work::Layout) {
                let layout = self.tree.layout(self.nodes[i]).unwrap();
                *shape = Rect::new(
                    layout.location.x as f64,
                    layout.location.y as f64,
                    (layout.location.x + layout.size.width) as f64,
                    (layout.location.y + layout.size.height) as f64,
                )
                .to_path(0.1);
            }
        }
        black_box(&self.scene);
    }
}
#[derive(Default)]
struct Timing {
    mutate: u128,
    driver: u128,
    apply: u128,
    layout: u128,
    paint: u128,
    total: u128,
    visited: usize,
    effects: usize,
    allocations: usize,
    bytes: usize,
}
fn percentile(v: &mut [u128], p: usize) -> f64 {
    v.sort_unstable();
    v[(v.len() - 1) * p / 100] as f64 / 1000.
}
fn report(n: usize, k: usize, work: Work, driver: Driver, rows: &[Timing]) {
    let p =
        |f: fn(&Timing) -> u128, pct| percentile(&mut rows.iter().map(f).collect::<Vec<_>>(), pct);
    println!(
        "{},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{},{}",
        n,
        k,
        work.name(),
        driver.name(),
        p(|x| x.total, 50),
        p(|x| x.total, 95),
        p(|x| x.mutate, 50),
        p(|x| x.driver, 50),
        p(|x| x.apply, 50),
        p(|x| x.layout, 50),
        p(|x| x.paint, 50),
        p(|x| x.total, 99),
        rows[0].visited,
        rows[0].effects,
        rows.iter().map(|x| x.allocations).sum::<usize>() / rows.len(),
        rows.iter().map(|x| x.bytes).sum::<usize>() / rows.len()
    );
}
fn run(n: usize, k: usize, work: Work, driver: Driver) -> Vec<plico_renderer::PaintCommand> {
    let retained = std::env::var("PLICO_PAINT").as_deref() == Ok("retained");
    let mut shared = Shared::new(n);
    let mut state = vec![0u8; n];
    let events = Rc::new(RefCell::new(Vec::<(usize, u8)>::with_capacity(n)));
    let mut signals: Vec<Signal<u8>> = Vec::with_capacity(n);
    let root = create_root(|| {
        if matches!(driver, Driver::Reactive) {
            for i in 0..n {
                let signal = create_signal(0u8);
                signals.push(signal);
                let events = Rc::clone(&events);
                create_effect(move || {
                    events.borrow_mut().push((i, signal.get()));
                });
            }
        }
    });
    events.borrow_mut().clear();
    let mut updates = Vec::with_capacity(n);
    let mut rows = Vec::with_capacity(SAMPLES);
    for sample in 0..(WARMUP + SAMPLES) {
        let ids: Vec<usize> = (0..k).map(|j| (j * 7919 + sample * 17) % n).collect();
        // IDs are unique for the tested sizes; mutations and event selection are precomputed.
        ALLOCS.store(0, Relaxed);
        BYTES.store(0, Relaxed);
        COUNT.store(std::env::var_os("PLICO_COUNT_ALLOC").is_some(), Relaxed);
        let started = Instant::now();
        let mut t = Timing::default();
        let start = Instant::now();
        if matches!(driver, Driver::Reactive) {
            root.run_in(|| {
                batch(|| {
                    for &i in &ids {
                        let next = 1 - signals[i].get_untracked();
                        signals[i].set(next);
                    }
                })
            });
        } else {
            for &i in &ids {
                state[i] ^= 1;
            }
        }
        t.mutate = start.elapsed().as_nanos();
        let start = Instant::now();
        updates.clear();
        match driver {
            Driver::Naive => {
                let fresh: Vec<_> = state.iter().copied().enumerate().collect();
                t.visited = n;
                for (i, v) in fresh {
                    if shared.values[i] != v {
                        updates.push((i, v));
                    }
                }
            }
            Driver::Full => {
                t.visited = n;
                for (i, &v) in state.iter().enumerate() {
                    if shared.values[i] != v {
                        updates.push((i, v));
                    }
                }
            }
            Driver::Dirty => {
                t.visited = k;
                for &i in &ids {
                    updates.push((i, state[i]));
                }
            }
            Driver::Reactive => {
                let mut queue = events.borrow_mut();
                t.visited = queue.len();
                t.effects = queue.len();
                updates.extend(queue.drain(..));
            }
        }
        t.driver = start.elapsed().as_nanos();
        assert_eq!(updates.len(), k);
        let start = Instant::now();
        shared.apply(&updates, work);
        t.apply = start.elapsed().as_nanos();
        let start = Instant::now();
        shared.layout(work);
        t.layout = start.elapsed().as_nanos();
        let start = Instant::now();
        if retained {
            shared.paint_retained(&updates, work);
        } else {
            shared.paint_full();
        }
        t.paint = start.elapsed().as_nanos();
        t.total = started.elapsed().as_nanos();
        COUNT.store(false, Relaxed);
        t.allocations = ALLOCS.load(Relaxed);
        t.bytes = BYTES.load(Relaxed);
        if matches!(driver, Driver::Reactive) {
            for &i in &ids {
                state[i] ^= 1;
            }
        }
        assert_eq!(
            shared.values, state,
            "drivers must produce identical full UI state"
        );
        if sample >= WARMUP {
            rows.push(t);
        }
    }
    report(n, k, work, driver, &rows);
    if retained {
        let previous: Vec<_> = shared.scene.commands.iter().cloned().collect();
        shared.paint_full();
        assert_eq!(
            previous,
            shared.scene.commands.as_slice(),
            "retained commands must match full rebuild"
        );
    }
    root.dispose();
    shared.scene.commands.iter().cloned().collect()
}
fn plico_baseline(n: usize, k: usize) {
    let mut engine = plico::Engine::default();
    let mut values = vec![false; n];
    let mut times = Vec::new();
    for sample in 0..(WARMUP + SAMPLES) {
        for j in 0..k {
            let i = (j * 7919 + sample * 17) % n;
            values[i] = !values[i];
        }
        let start = Instant::now();
        let stats = engine
            .frame_declarations(|cx| {
                for (i, v) in values.iter().enumerate() {
                    cx.text(if *v { "On" } else { "Off" }).key(i as u64);
                }
            })
            .unwrap();
        let elapsed = start.elapsed().as_nanos();
        black_box(stats.visited);
        if sample >= WARMUP {
            assert_eq!(stats.visited, n);
            assert_eq!(stats.text_writes, k);
            assert_eq!((stats.created, stats.removed), (0, 0));
            times.push(elapsed);
        }
    }
    eprintln!(
        "plico_engine_declaration_only,n={n},k={k},p50_us={:.2},p95_us={:.2},visited={n}",
        percentile(&mut times.clone(), 50),
        percentile(&mut times, 95)
    );
}
// Isolated ownership stress test: create one effect per child scope, dispose all,
// then change the source and assert no disposed effect can run.
fn scope_churn(k: usize) {
    let mut samples = Vec::new();
    let mut allocation_counts = Vec::new();
    let notifications = Rc::new(std::cell::Cell::new(0usize));
    let mut signal = None;
    let root = create_root(|| signal = Some(create_signal(0usize)));
    let signal = signal.unwrap();
    for sample in 0..(WARMUP + SAMPLES) {
        let mut scopes = Vec::with_capacity(k);
        ALLOCS.store(0, Relaxed);
        BYTES.store(0, Relaxed);
        COUNT.store(std::env::var_os("PLICO_COUNT_ALLOC").is_some(), Relaxed);
        let started = Instant::now();
        root.run_in(|| {
            for _ in 0..k {
                let notifications = Rc::clone(&notifications);
                scopes.push(create_child_scope(move || {
                    create_effect(move || {
                        black_box(signal.get());
                        notifications.set(notifications.get() + 1);
                    })
                }));
            }
            for scope in scopes.drain(..) {
                scope.dispose();
            }
        });
        let elapsed = started.elapsed().as_nanos();
        COUNT.store(false, Relaxed);
        let allocations = ALLOCS.load(Relaxed);
        let bytes = BYTES.load(Relaxed);
        let before = notifications.get();
        root.run_in(|| signal.set(signal.get_untracked() + 1));
        assert_eq!(notifications.get(), before, "disposed effects must not run");
        if sample >= WARMUP {
            samples.push(elapsed);
            allocation_counts.push((allocations, bytes));
        }
    }
    let allocations =
        allocation_counts.iter().map(|v| v.0).sum::<usize>() / allocation_counts.len();
    let bytes = allocation_counts.iter().map(|v| v.1).sum::<usize>() / allocation_counts.len();
    eprintln!(
        "sycamore_scope_create_dispose,k={k},p50_us={:.2},p95_us={:.2},allocations={allocations},allocated_bytes={bytes}",
        percentile(&mut samples.clone(), 50),
        percentile(&mut samples, 95)
    );
    root.dispose();
}

fn main() {
    println!(
        "nodes,changed,work,driver,total_p50_us,total_p95_us,mutation_p50_us,dispatch_p50_us,apply_p50_us,layout_p50_us,paint_p50_us,total_p99_us,visited,effects,allocations,allocated_bytes"
    );
    scope_churn(100);
    scope_churn(1000);
    if std::env::var_os("PLICO_SCOPE_ONLY").is_some() {
        return;
    }
    let round = std::env::var("PLICO_ROUND")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let drivers = [Driver::Naive, Driver::Full, Driver::Dirty, Driver::Reactive];
    for n in [1000, 10000] {
        for k in [1, 10, 20, 50, 100, n / 2, n] {
            for work in [Work::Paint, Work::Layout] {
                let mut reference = None;
                for i in 0..drivers.len() {
                    let commands = run(n, k, work, drivers[(i + round) % drivers.len()]);
                    if let Some(expected) = &reference {
                        assert_eq!(&commands, expected, "final scenes must match");
                    } else {
                        reference = Some(commands);
                    }
                }
            }
            plico_baseline(n, k);
        }
    }
}
