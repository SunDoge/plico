//! Deterministic simulation, not renderer performance or a signal-library benchmark.
use super::*;
use crate::allocation_counts;
use std::{hint::black_box, time::Instant};

fn work(mut value: u64, rounds: usize) -> u64 {
    for _ in 0..rounds {
        value = black_box(
            value
                .wrapping_mul(6364136223846793005)
                .rotate_left(13)
                .wrapping_add(1),
        );
    }
    value
}
fn body(cx: &mut UiContext<'_>, value: u64, rounds: usize) -> bool {
    paint(cx, value, work(value, rounds))
}
fn paint(cx: &mut UiContext<'_>, value: u64, derived: u64) -> bool {
    let mut clicked = false;
    let _ = column(cx).gap(12.).padding(24.)
        + |cx| {
            text!(cx, "Value: {value:08}");
            text!(cx, "Derived: {derived:020}");
            clicked = button(cx).text("Increment").key("increment").clicked();
        };
    clicked
}
async fn component(ui: AsyncUi, input: Props<u64>, rounds: usize) {
    loop {
        ui.frame(|cx| {
            let value = input.get();
            if body(cx, value, rounds) {
                input.set(value + 1);
                cx.request_rebuild();
            }
        })
        .await;
    }
}

const MODELS: [&str; 4] = [
    "immediate",
    "immediate-matched-shape",
    "coroutine-full-replay",
    "immediate-cached-work",
];

struct Immediate {
    engine: Engine,
    values: Vec<u64>,
    cache: Option<Vec<Option<(u64, u64)>>>,
    matched: bool,
    rounds: usize,
}
impl Immediate {
    fn new(count: usize, rounds: usize, matched: bool) -> Self {
        Self {
            engine: Engine::default(),
            values: vec![0; count],
            cache: None,
            matched,
            rounds,
        }
    }
    fn update(&mut self, event: usize, dirty: usize) {
        for offset in 0..dirty {
            let index = (event + offset) % self.values.len();
            self.values[index] = event as u64 + 1;
        }
    }
    fn frame(&mut self) -> FrameStats {
        let values = &mut self.values;
        let cache = &mut self.cache;
        let rounds = self.rounds;
        let matched = self.matched;
        self.engine
            .frame(|cx| {
                let _ = column(cx).key("forest").gap(10.)
                    + |cx| {
                        for (index, value) in values.iter_mut().enumerate() {
                            let derived = if let Some(cache) = cache {
                                match cache[index] {
                                    Some((version, result)) if version == *value => result,
                                    _ => {
                                        let result = work(*value, rounds);
                                        cache[index] = Some((*value, result));
                                        result
                                    }
                                }
                            } else {
                                work(*value, rounds)
                            };
                            if matched {
                                let _ = column(cx).key(index).gap(4.)
                                    + |cx| {
                                        let _ = column(cx).key("content")
                                            + |cx| {
                                                if paint(cx, *value, derived) {
                                                    *value += 1;
                                                    cx.request_rebuild();
                                                }
                                            };
                                    };
                            } else {
                                cx.keyed(index, |cx| {
                                    if paint(cx, *value, derived) {
                                        *value += 1;
                                        cx.request_rebuild();
                                    }
                                });
                            }
                        }
                    };
            })
            .unwrap()
    }
    fn click(&mut self, index: usize) -> usize {
        fn find(engine: &Engine, parent: Entity) -> Option<Entity> {
            let node = engine.node(parent).unwrap();
            if matches!(node.kind, Kind::Button | Kind::ButtonBase) {
                return Some(parent);
            }
            node.children.iter().find_map(|&id| find(engine, id))
        }
        let forest = self.engine.find(self.engine.root(), "forest").unwrap();
        let component = self.engine.node(forest).unwrap().children[index];
        self.engine
            .queue_click(find(&self.engine, component).unwrap());
        let first = self.frame();
        assert!(self.engine.changes().contains(plico_core::Changes::VIEW));
        first.visited + self.frame().visited
    }
}
struct Coroutine {
    scopes: Scopes,
    shared: Composite,
    inputs: Vec<Props<u64>>,
}
impl Coroutine {
    fn new(count: usize, rounds: usize) -> Self {
        let inputs = Rc::new(RefCell::new(Vec::with_capacity(count)));
        let mut scopes = Scopes::default();
        for _ in 0..count {
            let output = inputs.clone();
            scopes.mount(None, move |ui| {
                let input = Props::new(&ui, 0);
                output.borrow_mut().push(input.clone());
                Box::pin(component(ui, input, rounds))
            });
        }
        let inputs = inputs.borrow().clone();
        Self {
            scopes,
            shared: Composite::default(),
            inputs,
        }
    }
    fn update(&self, event: usize, dirty: usize) {
        for offset in 0..dirty {
            self.inputs[(event + offset) % self.inputs.len()].set(event as u64 + 1);
        }
    }
    fn frame(&mut self) -> FrameStats {
        self.scopes.pump();
        self.shared.commit(&self.scopes)
    }
    fn declarations(&self) -> usize {
        self.scopes
            .nodes
            .iter()
            .map(|(_, s)| s.host.ui.session.borrow().declarations)
            .sum()
    }
}

struct Measurement {
    ns: u128,
    allocations: usize,
    bytes: usize,
    visited: usize,
    declarations: usize,
}
fn measure(run: impl FnOnce() -> (usize, usize)) -> Measurement {
    allocation_counts::start();
    let start = Instant::now();
    let (visited, declarations) = run();
    let ns = start.elapsed().as_nanos();
    let (allocations, bytes) = allocation_counts::stop();
    Measurement {
        ns,
        allocations,
        bytes,
        visited,
        declarations,
    }
}
fn row(model: &str, count: usize, rounds: usize, scenario: &str, mut samples: Vec<Measurement>) {
    samples.sort_by_key(|m| m.ns);
    let mid = samples.len() / 2;
    let n = samples.len();
    let p95 = &samples[(n * 95 / 100).min(n - 1)];
    let allocs: usize = samples.iter().map(|m| m.allocations).sum();
    let bytes: usize = samples.iter().map(|m| m.bytes).sum();
    let visited: usize = samples.iter().map(|m| m.visited).sum();
    let declarations: usize = samples.iter().map(|m| m.declarations).sum();
    println!(
        "{model},{count},{rounds},{scenario},{},{},{:.3},{:.3},{},{}",
        samples[mid].ns,
        p95.ns,
        allocs as f64 / n as f64,
        bytes as f64 / n as f64,
        visited / n,
        declarations / n
    );
}

pub(super) fn run() {
    if cfg!(debug_assertions) {
        panic!("run benchmark with --release");
    }
    eprintln!(
        "simulation: 21 samples, 5 warmups, nanoseconds; allocated bytes are cumulative requests, not retained memory"
    );
    println!(
        "model,components,work_rounds,scenario,median_ns,p95_ns,alloc_calls,allocated_bytes,shared_nodes_visited,business_components_executed"
    );
    for count in [100, 1000] {
        for rounds in [0, 64, 1024, 8192] {
            for (scenario, dirty) in [
                ("forced-stable-replay", 0),
                ("sparse-one", 1),
                ("batch-10pct", count / 10),
                ("dense-all", count),
            ] {
                // Rotate order to reduce always-running-one-model-first bias.
                for offset in 0..4 {
                    let model = (offset + rounds + dirty) % 4;
                    if model != 2 {
                        let mut app = Immediate::new(count, rounds, model == 1);
                        if model == 3 {
                            app.cache = Some(vec![None; count]);
                        }
                        for _ in 0..5 {
                            app.frame();
                        }
                        let samples = (0..21)
                            .map(|event| {
                                measure(|| {
                                    app.update(event, dirty);
                                    let stats = app.frame();
                                    (stats.visited, count)
                                })
                            })
                            .collect();
                        row(MODELS[model], count, rounds, scenario, samples);
                    } else {
                        let mut app = Coroutine::new(count, rounds);
                        for _ in 0..5 {
                            app.frame();
                        }
                        let samples = (0..21)
                            .map(|event| {
                                let before = app.declarations();
                                let sample = measure(|| {
                                    app.update(event, dirty);
                                    let stats = app.frame();
                                    (stats.visited, dirty)
                                });
                                assert_eq!(app.declarations() - before, dirty);
                                sample
                            })
                            .collect();
                        row("coroutine-full-replay", count, rounds, scenario, samples);
                    }
                }
            }
            for (model, _) in MODELS.iter().enumerate() {
                if model != 2 {
                    let mut app = Immediate::new(count, rounds, model == 1);
                    if model == 3 {
                        app.cache = Some(vec![None; count]);
                    }
                    for _ in 0..5 {
                        app.frame();
                    }
                    let samples = (0..21)
                        .map(|event| measure(|| (app.click(event % count), count * 2)))
                        .collect();
                    row(MODELS[model], count, rounds, "click-one", samples);
                } else {
                    let mut app = Coroutine::new(count, rounds);
                    for _ in 0..5 {
                        app.frame();
                    }
                    let buttons: Vec<_> = app
                        .scopes
                        .nodes
                        .iter()
                        .map(|(id, _)| app.shared.button(id, "Increment"))
                        .collect();
                    let samples = (0..21)
                        .map(|event| {
                            let before = app.declarations();
                            let sample = measure(|| {
                                assert!(app.shared.click(&mut app.scopes, buttons[event % count]));
                                // Composite::click performs two full replay passes.
                                (app.shared.engine.last_stats().visited * 2, 2)
                            });
                            assert_eq!(app.declarations() - before, 2);
                            sample
                        })
                        .collect();
                    row("coroutine-full-replay", count, rounds, "click-one", samples);
                }
            }
        }
    }
    // Measure cold construction separately; destruction is intentionally excluded.
    for count in [100, 1000] {
        for (model, _) in MODELS.iter().enumerate() {
            let mut samples = Vec::new();
            for _ in 0..7 {
                if model != 2 {
                    let mut app = None;
                    samples.push(measure(|| {
                        let mut created = Immediate::new(count, 0, model == 1);
                        if model == 3 {
                            created.cache = Some(vec![None; count]);
                        }
                        let stats = created.frame();
                        app = Some(created);
                        (stats.visited, count)
                    }));
                    drop(app);
                } else {
                    let mut app = None;
                    samples.push(measure(|| {
                        let mut created = Coroutine::new(count, 0);
                        let stats = created.frame();
                        app = Some(created);
                        (stats.visited, count)
                    }));
                    drop(app);
                }
            }
            row(MODELS[model], count, 0, "cold-mount", samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn texts(engine: &Engine) -> Vec<String> {
        fn visit(engine: &Engine, parent: Entity, out: &mut Vec<String>) {
            for &id in engine.node(parent).unwrap().children {
                let node = engine.node(id).unwrap();
                if node.kind == Kind::Text {
                    out.push(node.text.to_owned());
                }
                visit(engine, id, out);
            }
        }
        let mut out = Vec::new();
        visit(engine, engine.root(), &mut out);
        out
    }
    #[test]
    fn benchmark_models_render_identical_text_after_sparse_and_dense_updates() {
        let mut immediate = Immediate::new(10, 64, false);
        let mut matched = Immediate::new(10, 64, true);
        let mut cached = Immediate::new(10, 64, false);
        cached.cache = Some(vec![None; 10]);
        let mut coroutine = Coroutine::new(10, 64);
        for (event, dirty) in [(0, 0), (1, 1), (2, 5), (3, 10)] {
            immediate.update(event, dirty);
            matched.update(event, dirty);
            cached.update(event, dirty);
            coroutine.update(event, dirty);
            immediate.frame();
            matched.frame();
            cached.frame();
            coroutine.frame();
            assert_eq!(texts(&immediate.engine), texts(&matched.engine));
            assert_eq!(texts(&immediate.engine), texts(&cached.engine));
            assert_eq!(texts(&immediate.engine), texts(&coroutine.shared.engine));
        }
        immediate.click(3);
        matched.click(3);
        cached.click(3);
        let scope = coroutine.scopes.nodes.iter().nth(3).unwrap().0;
        let button = coroutine.shared.button(scope, "Increment");
        coroutine.shared.click(&mut coroutine.scopes, button);
        assert_eq!(texts(&immediate.engine), texts(&matched.engine));
        assert_eq!(texts(&immediate.engine), texts(&cached.engine));
        assert_eq!(texts(&immediate.engine), texts(&coroutine.shared.engine));
    }
}
