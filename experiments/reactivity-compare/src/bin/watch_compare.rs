//! Same-process explicit watch / Sycamore comparison with matching retained text output.
use plico::{Entity, UiContext, VirtualClock};
use plico_reactivity_compare::{RegionId, RegionUi, Runtime, State};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{Source, View};
use slotmap::SecondaryMap;
use std::{
    cell::RefCell,
    hint::black_box,
    rc::Rc,
    time::{Duration, Instant},
};
use sycamore_reactive::{RootHandle, Signal, batch, create_effect, create_root, create_signal};

const OFF: &str = "First value";
const ON: &str = "Other value";
fn draw(cx: &mut RegionUi, value: bool) {
    cx.text(if value { ON } else { OFF });
}
trait Graph {
    fn change(&mut self, ids: &[usize]) -> Vec<usize>;
    fn with_output<R>(&self, index: usize, f: impl FnOnce(&[String]) -> R) -> R;
}
struct Explicit {
    runtime: Runtime,
    states: Vec<State<bool>>,
    regions: Vec<RegionId>,
    indices: SecondaryMap<RegionId, usize>,
}
impl Explicit {
    fn new(n: usize) -> Self {
        let mut runtime = Runtime::default();
        let mut states = Vec::with_capacity(n);
        let mut regions = Vec::with_capacity(n);
        let mut indices = SecondaryMap::new();
        for i in 0..n {
            let state = runtime.state(|| false);
            let region = runtime.watch((state,), |cx, value| draw(cx, *value.read()));
            states.push(state);
            regions.push(region);
            indices.insert(region, i);
        }
        assert_eq!(runtime.flush().len(), n);
        Self {
            runtime,
            states,
            regions,
            indices,
        }
    }
}
impl Graph for Explicit {
    fn change(&mut self, ids: &[usize]) -> Vec<usize> {
        for &i in ids {
            self.runtime.access(self.states[i]).update(|v| *v = !*v);
        }
        self.runtime
            .flush()
            .into_iter()
            .map(|id| self.indices[id])
            .collect()
    }
    fn with_output<R>(&self, i: usize, f: impl FnOnce(&[String]) -> R) -> R {
        f(self.runtime.output(self.regions[i]).unwrap())
    }
}
struct Sycamore {
    root: RootHandle,
    signals: Vec<Signal<bool>>,
    output: Rc<RefCell<Vec<Vec<String>>>>,
    queue: Rc<RefCell<Vec<usize>>>,
}
impl Sycamore {
    fn new(n: usize) -> Self {
        let output = Rc::new(RefCell::new(vec![Vec::new(); n]));
        let queue = Rc::new(RefCell::new(Vec::with_capacity(n)));
        let mut signals = Vec::with_capacity(n);
        let root = create_root(|| {
            for i in 0..n {
                let signal = create_signal(false);
                signals.push(signal);
                let output = Rc::clone(&output);
                let queue = Rc::clone(&queue);
                create_effect(move || {
                    let mut cx = RegionUi::default();
                    draw(&mut cx, signal.get());
                    let mut retained = output.borrow_mut();
                    if retained[i] != cx.text {
                        retained[i] = cx.text;
                        queue.borrow_mut().push(i);
                    }
                });
            }
        });
        assert_eq!(queue.borrow().len(), n);
        queue.borrow_mut().clear();
        Self {
            root,
            signals,
            output,
            queue,
        }
    }
}
impl Graph for Sycamore {
    fn change(&mut self, ids: &[usize]) -> Vec<usize> {
        self.root.run_in(|| {
            batch(|| {
                for &i in ids {
                    self.signals[i].set(!self.signals[i].get_untracked());
                }
            })
        });
        self.queue.borrow_mut().drain(..).collect()
    }
    fn with_output<R>(&self, i: usize, f: impl FnOnce(&[String]) -> R) -> R {
        f(&self.output.borrow()[i])
    }
}
impl Drop for Sycamore {
    fn drop(&mut self) {
        self.root.dispose();
    }
}
struct Screen {
    values: Vec<bool>,
}
impl View for Screen {
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
#[derive(Clone, Copy)]
struct Sample {
    update: u128,
    total: u128,
}
fn step(
    graph: &mut impl Graph,
    driver: &mut Option<Driver>,
    ids: &[usize],
    text: &mut TextContext,
) -> (Sample, Vec<usize>) {
    let start = Instant::now();
    let changes = graph.change(ids);
    if let Some(driver) = driver {
        for &i in &changes {
            graph.with_output(i, |output| {
                let value = &output[0];
                driver.source.view.values[i] = value == ON;
                assert_eq!(
                    driver
                        .source
                        .experiment_patch_text(driver.entities[i], value),
                    Some(true)
                );
            });
        }
    }
    let update = start.elapsed().as_nanos();
    if let Some(driver) = driver {
        driver.prepare(text);
    }
    (
        Sample {
            update,
            total: start.elapsed().as_nanos(),
        },
        changes,
    )
}
fn percentile(rows: &[Sample], f: impl Fn(&Sample) -> u128, p: usize) -> f64 {
    let mut values: Vec<_> = rows.iter().map(f).collect();
    values.sort_unstable();
    values[(values.len() - 1) * p / 100] as f64 / 1000.
}
fn verify(a: &Explicit, b: &Sycamore, expected: &[bool]) {
    for (i, &value) in expected.iter().enumerate() {
        let expected = if value { ON } else { OFF };
        a.with_output(i, |out| assert_eq!(out, &[expected]));
        b.with_output(i, |out| assert_eq!(out, &[expected]));
    }
}
fn run(n: usize, k: usize, pipeline: bool) {
    let mut text = TextContext::new();
    let mut a = Explicit::new(n);
    let mut b = Sycamore::new(n);
    let mut da = pipeline.then(|| Driver::new(n, &mut text));
    let mut db = pipeline.then(|| Driver::new(n, &mut text));
    let mut expected = vec![false; n];
    let (warmup, samples) = if pipeline { (5, 30) } else { (10, 100) };
    let mut ar = Vec::new();
    let mut br = Vec::new();
    for frame in 0..warmup + samples {
        let ids: Vec<_> = (0..k).map(|j| (j * 7919 + frame * 17) % n).collect();
        for &i in &ids {
            expected[i] ^= true;
        }
        let ((x, mut xc), (y, mut yc)) = if frame % 2 == 0 {
            (
                step(&mut a, &mut da, &ids, &mut text),
                step(&mut b, &mut db, &ids, &mut text),
            )
        } else {
            let y = step(&mut b, &mut db, &ids, &mut text);
            (step(&mut a, &mut da, &ids, &mut text), y)
        };
        xc.sort_unstable();
        yc.sort_unstable();
        assert_eq!(xc, yc);
        assert_eq!(xc.len(), k);
        verify(&a, &b, &expected);
        if let (Some(da), Some(db)) = (&mut da, &mut db) {
            assert_eq!(da.source.view.values, expected);
            assert_eq!(db.source.view.values, expected);
            if n <= 100 {
                da.scene(&mut text);
                db.scene(&mut text);
                assert_eq!(da.scene.commands, db.scene.commands);
            }
        }
        if frame >= warmup {
            ar.push(x);
            br.push(y);
        }
    }
    if let (Some(da), Some(db)) = (&mut da, &mut db) {
        da.scene(&mut text);
        db.scene(&mut text);
        assert_eq!(da.scene.commands, db.scene.commands);
    }
    for (mode, rows) in [("explicit_watch", ar), ("sycamore", br)] {
        println!(
            "{},{n},{k},{mode},{:.2},{:.2},{:.2}",
            if pipeline { "plico_cpu" } else { "graph_only" },
            percentile(&rows, |r| r.update, 50),
            percentile(&rows, |r| r.total, 50),
            percentile(&rows, |r| r.total, 95)
        );
    }
    black_box((a, b));
}
fn main() {
    println!("scope,nodes,changed,driver,update_p50_us,total_p50_us,total_p95_us");
    for pipeline in [false, true] {
        run(80, 10, pipeline);
        for n in [1000, 10000] {
            let mut counts = vec![1, 10, 100, n / 2, n];
            counts.sort_unstable();
            counts.dedup();
            for k in counts {
                run(n, k, pipeline);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batches_that_cancel_produce_no_patch_in_both_graphs() {
        let mut a = Explicit::new(4);
        let mut b = Sycamore::new(4);
        assert!(a.change(&[1, 1]).is_empty());
        assert!(b.change(&[1, 1]).is_empty());
        verify(&a, &b, &[false; 4]);
    }
    #[test]
    fn duplicate_invalidations_replay_final_values_once() {
        let mut a = Explicit::new(4);
        let mut b = Sycamore::new(4);
        assert_eq!(a.change(&[1, 1, 1]), [1]);
        assert_eq!(b.change(&[1, 1, 1]), [1]);
        verify(&a, &b, &[false, true, false, false]);
    }
}
