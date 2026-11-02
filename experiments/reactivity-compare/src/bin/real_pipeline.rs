//! Compare full declarations and Sycamore-driven text patches through the same Plico Source.
use plico::{Color, Entity, UiContext, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{Source, HostView};
use std::{
    cell::RefCell,
    hint::black_box,
    rc::Rc,
    time::{Duration, Instant},
};
use sycamore_reactive::{Signal, batch, create_effect, create_root, create_signal};

const WARMUP: usize = 5;
const SAMPLES: usize = 20;
const OFF: &str = "First value";
const ON: &str = "Other value";
#[derive(Clone, Copy, Debug)]
enum Work {
    Text,
    Color,
}
fn color(on: bool) -> Color {
    Color::Literal(if on { 0x0055aa } else { 0x222222 })
}
struct Screen {
    values: Vec<bool>,
    work: Work,
}
impl HostView for Screen {
    fn show(&mut self, cx: &mut UiContext) {
        cx.column().key(999_999u64).children(|cx| {
            for (i, &on) in self.values.iter().enumerate() {
                cx.text(if matches!(self.work, Work::Text) && on {
                    ON
                } else {
                    OFF
                })
                .color(color(matches!(self.work, Work::Color) && on))
                .key(i as u64)
                .font_size(14);
            }
        });
    }
}
struct Driver {
    source: Source<Screen>,
    scene: Scene,
    clock: VirtualClock,
    entities: Vec<Entity>,
    size: (u32, u32),
}
impl Driver {
    fn new(n: usize, work: Work, text: &mut TextContext) -> Self {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(
            Screen {
                values: vec![false; n],
                work,
            },
            clock.clone(),
        );
        let mut scene = Scene::new();
        let size = (800, (n as u32) * 24 + 100);
        source
            .render_retained_frame(&mut scene, text, size, 1.)
            .unwrap();
        assert!(source.last_error.is_none());
        let root = source.engine.root();
        let column = source.engine.find(root, 999_999u64).unwrap();
        let entities = source.engine.node(column).unwrap().children.to_vec();
        assert_eq!(entities.len(), n);
        Self {
            source,
            scene,
            clock,
            entities,
            size,
        }
    }
    fn render(&mut self, text: &mut TextContext, with_scene: bool) {
        self.clock.advance(Duration::from_millis(16)).unwrap();
        if with_scene {
            self.render_scene(text);
        } else {
            self.source
                .experiment_prepare_without_scene(text, self.size.0, self.size.1);
            assert!(self.source.last_error.is_none());
        }
    }
    fn render_scene(&mut self, text: &mut TextContext) {
        self.source
            .render_retained_frame(&mut self.scene, text, self.size, 1.)
            .unwrap();
        assert!(self.source.last_error.is_none());
        black_box(&self.scene);
    }
}
#[derive(Default)]
struct Sample {
    total: u128,
    update: u128,
    render: u128,
    declaration: u128,
    sync: u128,
    layout: u128,
    visits: usize,
    layout_runs: usize,
    flatten: usize,
    scene_builds: usize,
}
fn percentile(values: &mut [u128], p: usize) -> f64 {
    values.sort_unstable();
    values[(values.len() - 1) * p / 100] as f64 / 1000.
}
fn report(n: usize, k: usize, work: Work, mode: &str, values: &[Sample]) {
    let p =
        |f: fn(&Sample) -> u128, pc| percentile(&mut values.iter().map(f).collect::<Vec<_>>(), pc);
    println!(
        "{work:?},{n},{k},{mode},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{},{}",
        p(|s| s.total, 50),
        p(|s| s.total, 95),
        p(|s| s.update, 50),
        p(|s| s.render, 50),
        p(|s| s.declaration, 50),
        p(|s| s.sync, 50),
        p(|s| s.layout, 50),
        values.iter().map(|s| s.visits).sum::<usize>() / values.len(),
        values.iter().map(|s| s.layout_runs).sum::<usize>() / values.len(),
        values.iter().map(|s| s.flatten).sum::<usize>() / values.len(),
        values.iter().map(|s| s.scene_builds).sum::<usize>() / values.len()
    );
}
fn full_step(
    driver: &mut Driver,
    ids: &[usize],
    text: &mut TextContext,
    with_scene: bool,
) -> Sample {
    let start = Instant::now();
    for &i in ids {
        driver.source.view.values[i] ^= true;
    }
    driver.source.request_rebuild();
    let update = start.elapsed().as_nanos();
    let render_start = Instant::now();
    driver.render(text, with_scene);
    let render = render_start.elapsed().as_nanos();
    let stats = driver.source.adapter.stats;
    Sample {
        total: start.elapsed().as_nanos(),
        update,
        render,
        declaration: driver.source.declaration_micros * 1000,
        sync: stats.sync_micros * 1000,
        layout: if stats.layout_runs == 0 {
            0
        } else {
            stats.layout_micros * 1000
        },
        visits: stats.sync_visits,
        layout_runs: stats.layout_runs,
        flatten: stats.flatten_visits,
        scene_builds: stats.scene_builds,
    }
}
fn reactive_step(
    driver: &mut Driver,
    ids: &[usize],
    root: &sycamore_reactive::RootHandle,
    signals: &[Signal<bool>],
    queue: &Rc<RefCell<Vec<(usize, bool)>>>,
    text: &mut TextContext,
    with_scene: bool,
) -> Sample {
    let start = Instant::now();
    root.run_in(|| {
        batch(|| {
            for &i in ids {
                signals[i].set(!signals[i].get_untracked());
            }
        })
    });
    let mut pending = queue.borrow_mut();
    assert_eq!(pending.len(), ids.len());
    for (i, value) in pending.drain(..) {
        driver.source.view.values[i] = value;
        let changed = match driver.source.view.work {
            Work::Text => driver
                .source
                .experiment_patch_text(driver.entities[i], if value { ON } else { OFF }),
            Work::Color => driver
                .source
                .experiment_patch_color(driver.entities[i], color(value)),
        };
        assert_eq!(changed, Some(true));
    }
    drop(pending);
    let update = start.elapsed().as_nanos();
    let render_start = Instant::now();
    driver.render(text, with_scene);
    let render = render_start.elapsed().as_nanos();
    let stats = driver.source.adapter.stats;
    Sample {
        total: start.elapsed().as_nanos(),
        update,
        render,
        declaration: 0,
        sync: stats.sync_micros * 1000,
        layout: if stats.layout_runs == 0 {
            0
        } else {
            stats.layout_micros * 1000
        },
        visits: stats.sync_visits,
        layout_runs: stats.layout_runs,
        flatten: stats.flatten_visits,
        scene_builds: stats.scene_builds,
    }
}
fn run(n: usize, k: usize, work: Work) {
    let with_scene = std::env::var_os("PLICO_WITH_SCENE").is_some();
    let mut text = TextContext::new();
    let mut immediate = Driver::new(n, work, &mut text);
    let mut reactive = Driver::new(n, work, &mut text);
    let queue = Rc::new(RefCell::new(Vec::with_capacity(n)));
    let mut signals = Vec::with_capacity(n);
    let root = create_root(|| {
        for i in 0..n {
            let signal = create_signal(false);
            signals.push(signal);
            let queue = Rc::clone(&queue);
            create_effect(move || queue.borrow_mut().push((i, signal.get())));
        }
    });
    queue.borrow_mut().clear();
    let mut full_rows = Vec::with_capacity(SAMPLES);
    let mut reactive_rows = Vec::with_capacity(SAMPLES);
    for frame in 0..(WARMUP + SAMPLES) {
        let ids: Vec<_> = (0..k).map(|j| (j * 7919 + frame * 17) % n).collect();
        let (full, patch) = if frame % 2 == 0 {
            (
                full_step(&mut immediate, &ids, &mut text, with_scene),
                reactive_step(
                    &mut reactive,
                    &ids,
                    &root,
                    &signals,
                    &queue,
                    &mut text,
                    with_scene,
                ),
            )
        } else {
            let patch = reactive_step(
                &mut reactive,
                &ids,
                &root,
                &signals,
                &queue,
                &mut text,
                with_scene,
            );
            (
                full_step(&mut immediate, &ids, &mut text, with_scene),
                patch,
            )
        };
        assert_eq!(immediate.source.view.values, reactive.source.view.values);
        for &i in &ids {
            assert_eq!(
                immediate
                    .source
                    .engine
                    .node(immediate.entities[i])
                    .unwrap()
                    .text,
                reactive
                    .source
                    .engine
                    .node(reactive.entities[i])
                    .unwrap()
                    .text
            );
        }
        if n <= 100 {
            if !with_scene {
                immediate.render_scene(&mut text);
                reactive.render_scene(&mut text);
            }
            assert_eq!(
                immediate.scene.commands, reactive.scene.commands,
                "scene mismatch after update {frame}"
            );
        }
        if frame >= WARMUP {
            full_rows.push(full);
            reactive_rows.push(patch);
        }
    }
    if !with_scene {
        immediate.render_scene(&mut text);
        reactive.render_scene(&mut text);
    }
    assert_eq!(
        immediate.scene.commands, reactive.scene.commands,
        "final scene mismatch"
    );
    report(n, k, work, "immediate", &full_rows);
    report(n, k, work, "sycamore_patch", &reactive_rows);
    root.dispose();
}
fn main() {
    println!(
        "work,nodes,changed,driver,total_p50_us,total_p95_us,update_p50_us,prepare_p50_us,declaration_p50_us,sync_p50_us,layout_p50_us,sync_visits,layout_runs,flatten_visits,scene_builds"
    );
    for work in [Work::Text, Work::Color] {
        run(80, 10, work);
        for n in [1000, 10000] {
            let mut counts = vec![1, 10, 100, n / 10, n / 5, n / 2, n];
            counts.sort_unstable();
            counts.dedup();
            for k in counts {
                run(n, k, work);
            }
        }
    }
}
