//! Aggregate selected phase spans in memory; no per-node tracing or per-frame I/O.
use super::thread_cpu_ns;
use std::{collections::BTreeMap, sync::Mutex, time::Instant};
use tracing_subscriber::{Layer, layer::Context, prelude::*, registry::LookupSpan};
const NAMES: &[&str] = &[
    "frame",
    "frame.retained",
    "frame.declare",
    "frame.present",
    "layout.compute",
    "layout.flatten",
    "backend.index",
    "backend.presentation",
    "backend.geometry_post",
    "skia.wait_fence",
    "skia.acquire",
    "skia.damage",
    "skia.draw",
    "skia.replay",
    "skia.flush_submit",
    "skia.queue_present",
];
struct Collector {
    start: Instant,
    totals: Mutex<BTreeMap<&'static str, (u64, u128, u128)>>,
}
struct Timer(Instant, u128);

impl<S: tracing::Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Collector {
    fn on_record(&self, id: &tracing::Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        if self.start.elapsed().as_secs() < 2 || ctx.span(id).unwrap().name() != "frame" {
            return;
        }
        struct Fields<'a>(&'a mut BTreeMap<&'static str, (u64, u128, u128)>);
        impl tracing::field::Visit for Fields<'_> {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
            fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
                let name = match field.name() {
                    "build_ms" => "frame.build_timer",
                    "scene_ms" => "frame.scene_timer",
                    _ => return,
                };
                let entry = self.0.entry(name).or_default();
                entry.0 += 1;
                entry.1 += (value * 1_000_000.) as u128;
            }
        }
        values.record(&mut Fields(&mut self.totals.lock().unwrap()));
    }
    fn on_enter(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        ctx.span(id)
            .unwrap()
            .extensions_mut()
            .insert(Timer(Instant::now(), thread_cpu_ns()));
    }
    fn on_exit(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let (start, cpu_start) = {
            let extensions = span.extensions();
            let timer = extensions.get::<Timer>().unwrap();
            (timer.0, timer.1)
        };
        let cpu_elapsed = thread_cpu_ns() - cpu_start;
        if start.duration_since(self.start).as_secs() < 2 {
            return;
        }
        let elapsed = start.elapsed().as_nanos();
        let mut totals = self.totals.lock().unwrap();
        let entry = totals.entry(span.name()).or_default();
        entry.0 += 1;
        entry.1 += elapsed;
        entry.2 += cpu_elapsed;
        if span.name() == "frame" && entry.0 == 300 {
            eprintln!("phase,count,mean_wall_us,mean_thread_cpu_us");
            for (name, (count, nanos, cpu)) in totals.iter() {
                let cpu = if name.ends_with("_timer") || !cfg!(target_os = "linux") {
                    "NA".to_owned()
                } else {
                    format!("{:.3}", *cpu as f64 / *count as f64 / 1000.)
                };
                eprintln!(
                    "{name},{count},{:.3},{cpu}",
                    *nanos as f64 / *count as f64 / 1000.
                );
            }
        }
    }
}
pub fn install() {
    tracing_subscriber::registry()
        .with(
            Collector {
                start: Instant::now(),
                totals: Mutex::new(BTreeMap::new()),
            }
            .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
                meta.is_span() && NAMES.contains(&meta.name())
            })),
        )
        .init();
}
