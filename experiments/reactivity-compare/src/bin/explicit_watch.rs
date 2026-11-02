//! Headless real-Plico demo and benchmark. No GPU or timed Scene encoding.
use plico::{Entity, UiContext, VirtualClock};
use plico_reactivity_compare::{RegionId, Runtime, State, watch};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{Source, HostView};
use slotmap::SecondaryMap;
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

const OFF: &str = "First value";
const ON: &str = "Other value";
struct Screen {
    values: Vec<bool>,
}
impl HostView for Screen {
    fn show(&mut self, cx: &mut UiContext) {
        cx.column().key(999_999u64).children(|cx| {
            for (i, &value) in self.values.iter().enumerate() {
                cx.text(if value { ON } else { OFF })
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
    fn new(n: usize, text: &mut TextContext) -> Self {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(
            Screen {
                values: vec![false; n],
            },
            clock.clone(),
        );
        let size = (800, n as u32 * 24 + 100);
        let mut scene = Scene::new();
        source
            .render_retained_frame(&mut scene, text, size, 1.)
            .unwrap();
        let column = source
            .engine
            .find(source.engine.root(), 999_999u64)
            .unwrap();
        let entities = source.engine.node(column).unwrap().children.to_vec();
        Self {
            source,
            scene,
            clock,
            entities,
            size,
        }
    }
    fn prepare(&mut self, text: &mut TextContext) {
        self.clock.advance(Duration::from_millis(16)).unwrap();
        self.source
            .experiment_prepare_without_scene(text, self.size.0, self.size.1);
        assert!(self.source.last_error.is_none());
    }
    fn scene(&mut self, text: &mut TextContext) {
        self.source
            .render_retained_frame(&mut self.scene, text, self.size, 1.)
            .unwrap();
    }
}
fn demo() {
    let mut rt = Runtime::default();
    let count = rt.state(|| 0);
    let step = rt.state(|| 2);
    let label = rt.watch((count,), |cx, count| {
        cx.text(format!("Count: {}", *count.read()));
    });
    let summary = rt.watch((count, step), |cx, count, step| {
        cx.text(format!("Count {}, step {}", *count.read(), *step.read()));
    });
    let increment = rt.on_click(label, (count,), |count| {
        count.update(|c| *c += 1);
    });
    let initial = rt.flush();
    assert_eq!(initial.len(), 2);
    eprintln!("initial: {:?}", rt.output(label).unwrap());
    assert!(rt.click(increment));
    assert!(rt.click(increment));
    assert_eq!(rt.flush().len(), 2); // One replay per region, two event writes.
    eprintln!("two clicks, one flush: {:?}", rt.output(summary).unwrap());
    let by_step = rt.on_click_rw(label, (step, count), |step, count| *count += *step);
    assert!(rt.click(by_step));
    assert_eq!(rt.flush().len(), 2);
    eprintln!("fn(&step, &mut count): {:?}", rt.output(label).unwrap());
    rt.unmount(label);
    assert!(!rt.click(increment));
    eprintln!("unmounted handler rejected: true");
}
#[derive(Clone, Copy)]
struct Sample {
    total: u128,
    frontend: u128,
    sync_visits: usize,
    layouts: usize,
    flatten: usize,
}
fn full(driver: &mut Driver, ids: &[usize], text: &mut TextContext) -> Sample {
    let start = Instant::now();
    for &i in ids {
        driver.source.view.values[i] ^= true;
    }
    driver.source.request_rebuild();
    let mutation = start.elapsed().as_nanos();
    driver.prepare(text);
    let total = start.elapsed().as_nanos();
    let s = driver.source.adapter.stats;
    Sample {
        total,
        frontend: mutation + driver.source.declaration_micros * 1000,
        sync_visits: s.sync_visits,
        layouts: s.layout_runs,
        flatten: s.flatten_visits,
    }
}
fn explicit(
    driver: &mut Driver,
    rt: &mut Runtime,
    states: &[State<bool>],
    bindings: &SecondaryMap<RegionId, (usize, Entity)>,
    ids: &[usize],
    text: &mut TextContext,
) -> Sample {
    let start = Instant::now();
    for &i in ids {
        rt.access(states[i]).update(|v| *v = !*v);
        driver.source.view.values[i] ^= true;
    }
    let changed = rt.flush();
    assert_eq!(changed.len(), ids.len());
    for &region in &changed {
        let (_, entity) = bindings[region];
        let output = rt.output(region).unwrap();
        assert_eq!(
            driver.source.experiment_patch_text(entity, &output[0]),
            Some(true)
        );
    }
    let frontend = start.elapsed().as_nanos();
    driver.prepare(text);
    let total = start.elapsed().as_nanos();
    let s = driver.source.adapter.stats;
    black_box(changed);
    Sample {
        total,
        frontend,
        sync_visits: s.sync_visits,
        layouts: s.layout_runs,
        flatten: s.flatten_visits,
    }
}
fn median(values: impl Iterator<Item = u128>) -> f64 {
    let mut values: Vec<_> = values.collect();
    values.sort_unstable();
    values[values.len() / 2] as f64 / 1000.
}
fn run(n: usize, k: usize) {
    let mut text = TextContext::new();
    let mut a = Driver::new(n, &mut text);
    let mut b = Driver::new(n, &mut text);
    let mut rt = Runtime::default();
    let mut states = Vec::new();
    let mut bindings = SecondaryMap::new();
    for i in 0..n {
        let s = rt.state(|| false);
        states.push(s);
        let region = watch!(rt; (s,), |cx, s| { cx.text(if *s.read() { ON } else { OFF }); });
        bindings.insert(region, (i, b.entities[i]));
    }
    assert_eq!(rt.flush().len(), n);
    let mut full_rows = Vec::new();
    let mut explicit_rows = Vec::new();
    for frame in 0..35 {
        let ids: Vec<_> = (0..k).map(|j| (j * 7919 + frame * 17) % n).collect();
        let (x, y) = if frame % 2 == 0 {
            (
                full(&mut a, &ids, &mut text),
                explicit(&mut b, &mut rt, &states, &bindings, &ids, &mut text),
            )
        } else {
            let y = explicit(&mut b, &mut rt, &states, &bindings, &ids, &mut text);
            (full(&mut a, &ids, &mut text), y)
        };
        assert_eq!(a.source.view.values, b.source.view.values);
        for &i in &ids {
            assert_eq!(
                a.source.engine.node(a.entities[i]).unwrap().text,
                b.source.engine.node(b.entities[i]).unwrap().text
            );
        }
        if n <= 100 {
            a.scene(&mut text);
            b.scene(&mut text);
            assert_eq!(a.scene.commands, b.scene.commands);
        }
        if frame >= 5 {
            full_rows.push(x);
            explicit_rows.push(y);
        }
    }
    a.scene(&mut text);
    b.scene(&mut text);
    assert_eq!(a.scene.commands, b.scene.commands);
    for (name, rows) in [("immediate", full_rows), ("explicit_watch", explicit_rows)] {
        println!(
            "{n},{k},{name},{:.2},{:.2},{},{},{}",
            median(rows.iter().map(|r| r.total)),
            median(rows.iter().map(|r| r.frontend)),
            rows[0].sync_visits,
            rows[0].layouts,
            rows[0].flatten
        );
    }
}
fn main() {
    demo();
    if std::env::var_os("PLICO_WATCH_DEMO_ONLY").is_some() {
        return;
    }
    println!(
        "nodes,changed,driver,total_p50_us,frontend_p50_us,sync_visits,layout_runs,flatten_visits"
    );
    run(80, 10);
    for n in [1000, 10000] {
        for k in [1, 100, n / 2, n] {
            run(n, k);
        }
    }
}
