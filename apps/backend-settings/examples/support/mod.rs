//! Optional phase-level tracing for CPU benchmarks. No per-node span traffic.
use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};
use tracing_subscriber::{Layer, layer::Context, prelude::*, registry::LookupSpan};
const NAMES: [&str; 8] = [
    "layout.compute",
    "layout.flatten",
    "backend.index",
    "backend.align",
    "backend.presentation",
    "backend.selection",
    "backend.assets",
    "backend.geometry_post",
];
static COUNTERS: OnceLock<Arc<Mutex<[u128; 8]>>> = OnceLock::new();
struct Collector(Arc<Mutex<[u128; 8]>>);
struct Timer(Instant);
impl<S: tracing::Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Collector {
    fn on_enter(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        ctx.span(id)
            .unwrap()
            .extensions_mut()
            .insert(Timer(Instant::now()));
    }
    fn on_exit(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let elapsed = span
            .extensions()
            .get::<Timer>()
            .unwrap()
            .0
            .elapsed()
            .as_nanos();
        if let Some(index) = NAMES.iter().position(|name| *name == span.name()) {
            self.0.lock().unwrap()[index] += elapsed;
        }
    }
}
pub fn install() {
    let counters = Arc::new(Mutex::new([0; 8]));
    COUNTERS.set(Arc::clone(&counters)).unwrap();
    tracing_subscriber::registry()
        .with(
            Collector(counters).with_filter(tracing_subscriber::filter::filter_fn(|meta| {
                meta.target() == "plico::layout"
            })),
        )
        .init();
}
pub fn reset() {
    *COUNTERS.get().unwrap().lock().unwrap() = [0; 8];
}
pub fn accumulate(totals: &mut [u128; 8]) {
    for (total, value) in totals
        .iter_mut()
        .zip(*COUNTERS.get().unwrap().lock().unwrap())
    {
        *total += value;
    }
}
pub fn report(label: &str, totals: [u128; 8], samples: usize) {
    eprint!("stages {label}");
    for (name, nanos) in NAMES.iter().zip(totals) {
        eprint!(" {name}_us={:.3}", nanos as f64 / samples as f64 / 1000.);
    }
    eprintln!();
}
