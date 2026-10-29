//! Shared text-grid and batch-download workloads through immediate and reactive APIs.
#[cfg(feature = "profiling")]
mod visible_texts_profile;

use plico::{Engine, Entity, FrameError, prelude::*};
use plico_renderer::{FrameSource, RenderBackend, Scene, TextContext, WakeCallback, WindowOptions};
use plico_runtime::{
    Source, View,
    regions::{Access, Reactive, Regions, ReplayBatch, State, WatchUi},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(usage_rs::Cli)]
#[usage(about = "Compare a visible text grid or download dashboard on the same Skia backend")]
struct Options {
    #[usage(long)]
    immediate: bool,
    /// Batch-download dashboard: 48 visible tasks with segmented progress meters.
    #[usage(long)]
    downloads: bool,
    #[usage(long)]
    headless: bool,
    /// Emit existing Skia cache counters outside timed samples (requires plico-skia/profiling).
    #[usage(long)]
    cache_stats: bool,
    #[usage(long, default = "60")]
    hz: usize,
    #[usage(long, default = "600")]
    updates: usize,
    #[usage(long, default = "100")]
    warmup: usize,
}
#[derive(Debug, snafu::Snafu)]
enum Error {
    #[snafu(display("invalid benchmark option: {message}"))]
    Option { message: &'static str },
    #[snafu(display("native window failed: {source}"))]
    Window { source: plico_shell::Error },
}
struct Feed {
    value: Arc<AtomicUsize>,
    wake: Arc<Mutex<Option<WakeCallback>>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Feed {
    fn new(hz: usize) -> Self {
        let value = Arc::new(AtomicUsize::new(0));
        let wake = Arc::new(Mutex::new(None::<WakeCallback>));
        let stopped = Arc::new(AtomicBool::new(false));
        let (v, w, stop) = (value.clone(), wake.clone(), stopped.clone());
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                thread::park_timeout(Duration::from_secs_f64(1. / hz as f64));
                if stop.load(Ordering::Acquire) {
                    break;
                }
                v.fetch_add(1, Ordering::AcqRel);
                let callback = w.lock().unwrap().clone();
                if let Some(callback) = callback {
                    callback();
                }
            }
        });
        Self {
            value,
            wake,
            stopped,
            worker: Some(worker),
        }
    }
}
impl Drop for Feed {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            worker.join().unwrap();
        }
    }
}
struct Texts {
    labels: Arc<Vec<String>>,
    value: usize,
    reactive: Option<(Regions, State<usize>)>,
    feed: Option<Feed>,
}
impl Texts {
    fn new(immediate: bool, downloads: bool) -> Self {
        let labels: Arc<Vec<String>> = Arc::new(if downloads {
            (0..48)
                .map(|i| format!("Dataset shard {i:02}.parquet"))
                .collect()
        } else {
            (0..500).map(|i| format!("{i:03}:00")).collect()
        });
        let reactive = if immediate {
            None
        } else {
            let mut handle = None;
            let regions = Regions::new(|app| {
                let value = app.state(|| 0_usize);
                let labels = app.state(|| labels.clone());
                handle = Some(value);
                app.root((value, labels), reactive_root);
            });
            Some((regions, handle.unwrap()))
        };
        Self {
            labels,
            value: 0,
            reactive,
            feed: None,
        }
    }
    fn set(&mut self, value: usize) {
        self.value = value;
        if let Some((regions, state)) = &self.reactive {
            regions.access(*state).set(value);
        }
    }
}
fn value_text(value: usize) -> String {
    format!("000:{:02}", value % 100)
}
#[component]
fn scaffold(cx: &mut UiContext<'_>, mut cell: impl FnMut(&mut UiContext<'_>, usize)) {
    let row_height = 19.5; // Fixed workload: 1440 x 1060, as in the MyGO window.
    column(cx).width_percent(100.).height_percent(100.).padding(14).gap(10).children(|cx| {
        column(cx).height(230.).shrink(0.).gap(10).children(|cx| {
            text(cx, "Plico sparse-update lab").font_size(24).weight(700);
            text(cx, "500 simultaneously visible Text nodes; only the first label changes.").font_size(13);
            text(cx, "Same tree, strings, sizes, clock driver and Skia backend for immediate / reactive.").font_size(13);
        });
        column(cx).gap(0).children(|cx| {
            for row_index in 0..20 {
                row(cx).key(row_index as u64).height(row_height).shrink(0.).gap(3).children(|cx| {
                    for column_index in 0..25 {
                        column(cx).key(column_index as u64).width(0.).grow(1.).height(row_height).children(|cx| {
                            // Row and column keys supply identical stable identity on both paths.
                            cell(cx, row_index * 25 + column_index);
                        });
                    }
                });
            }
        });
    });
}
// A typical many-task dashboard: immutable metadata and paused/completed tasks
// remain visible while one transfer's smoothed progress changes at display rate.
// Both models cache all immutable labels; there is no parsing/sorting in show.
#[component]
fn download_dashboard(
    cx: &mut UiContext<'_>,
    labels: &[String],
    value: usize,
    state: Option<State<usize>>,
) {
    column(cx).width_percent(100.).height_percent(100.).padding(14).gap(0).children(|cx| {
        text(cx, "Dataset downloads · 48 transfers").font_size(24).weight(700);
        text(cx, "One active transfer; paused/completed transfers stay visible. Simulated data, no network I/O.").font_size(13);
        for (i, label) in labels.iter().enumerate() {
            row(cx).key(i as u64).height(19.).shrink(0.).gap(8).children(|cx| {
                text(cx, label).width(240.).font_size(13);
                text(cx, if i == 0 { "Downloading" } else if i.is_multiple_of(3) { "Complete" } else { "Paused" }).width(100.).font_size(13);
                text(cx, "2.0 GiB").width(80.).font_size(13);
                column(cx).width(0.).grow(1.).children(|cx| {
                    if i == 0 {
                        if let Some(state) = state {
                            cx.watch_keyed(1, (state,), |cx, (value,)| download_meter(cx, *value.read()));
                        } else {
                            column(cx).children(|cx| download_meter(cx, value));
                        }
                    } else {
                        column(cx).children(|cx| download_meter(cx, if i.is_multiple_of(3) { 99 } else { i * 7 % 100 }));
                    }
                });
                if i == 0 {
                    if let Some(state) = state {
                        text(cx, "0%").width(50.).font_size(13).rx(state, |node, value| { node.text(format!("{}%", *value.read() % 100)); });
                    } else {
                        text(cx, format!("{}%", value % 100)).width(50.).font_size(13);
                    }
                } else {
                    text(cx, if i.is_multiple_of(3) { "100%" } else { "—" }).width(50.).font_size(13);
                }
                button(cx).text("Pause").ghost().height(18.).padding(0).font_size(13);
                button(cx).text("Cancel").ghost().height(18.).padding(0).font_size(13);
            });
        }
    });
}
#[component]
fn download_meter(cx: &mut UiContext<'_>, value: usize) {
    row(cx)
        .width_percent(100.)
        .height(12.)
        .gap(2)
        .children(|cx| {
            for segment in 0..32 {
                column(cx)
                    .key(segment as u64)
                    .width(0.)
                    .grow(1.)
                    .height(10.)
                    .radius(2.)
                    .background(if segment * 100 / 32 <= value % 100 {
                        Color::Accent
                    } else {
                        Color::Border
                    })
                    .children(|_| {});
            }
        });
}
fn reactive_root(
    cx: &mut UiContext<'_>,
    (value, labels): (Access<'_, usize>, Access<'_, Arc<Vec<String>>>),
) {
    let labels = labels.read();
    if labels.len() == 48 {
        download_dashboard(cx, &labels, 0, Some(value.handle()));
        return;
    }
    scaffold(cx, |cx, i| {
        if i == 0 {
            text(cx, "000:00")
                .font_size(13)
                .ellipsis(true)
                .color(Color::Accent)
                .rx(value.handle(), |node, value| {
                    node.text(value_text(*value.read()));
                });
        } else {
            text(cx, &labels[i]).font_size(13).ellipsis(true);
        }
    });
}
impl View for Texts {
    fn show(&mut self, cx: &mut UiContext<'_>) {
        if let Some((regions, _)) = &mut self.reactive {
            regions.show(cx);
        } else {
            let dynamic = value_text(self.value);
            column(cx)
                .key("regions-root")
                .width_percent(100.)
                .height_percent(100.)
                .children(|cx| {
                    if self.labels.len() == 48 {
                        download_dashboard(cx, &self.labels, self.value, None);
                        return;
                    }
                    scaffold(cx, |cx, i| {
                        if i == 0 {
                            text(cx, &dynamic)
                                .font_size(13)
                                .ellipsis(true)
                                .color(Color::Accent);
                        } else {
                            text(cx, &self.labels[i]).font_size(13).ellipsis(true);
                        }
                    });
                });
        }
    }
    fn scene_cache_roots(&self) -> Vec<Entity> {
        self.reactive
            .as_ref()
            .map_or_else(Vec::new, |(regions, _)| regions.scene_cache_roots())
    }
    fn replay_regions(&mut self, engine: &mut Engine) -> Option<Result<ReplayBatch, FrameError>> {
        self.reactive
            .as_mut()
            .and_then(|(regions, _)| regions.replay_regions(engine))
    }
    fn snapshot_changed(&self) -> bool {
        self.feed
            .as_ref()
            .is_some_and(|f| f.value.load(Ordering::Acquire) != self.value)
            || self
                .reactive
                .as_ref()
                .is_some_and(|(r, _)| r.snapshot_changed())
    }
    fn poll_reload(&mut self) -> bool {
        let latest = self
            .feed
            .as_ref()
            .map_or(self.value, |f| f.value.load(Ordering::Acquire));
        if latest == self.value {
            false
        } else {
            self.set(latest);
            true
        }
    }
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        if let Some(feed) = &self.feed {
            *feed.wake.lock().unwrap() = Some(wake);
        }
    }
}
fn prepare(source: &mut Source<Texts>, text: &mut TextContext, scene: &mut Scene) {
    source
        .render_retained_frame(scene, text, (1440, 1060), 1.)
        .unwrap();
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn bench(immediate: bool, options: &Options) -> Vec<(String, [f32; 4])> {
    let mut source = Source::new(Texts::new(immediate, options.downloads));
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    let mut surface = plico_skia::SkiaSurface::new(1440, 1060).unwrap();
    surface.configure_text(&mut text);
    let mut scene = Scene::new();
    prepare(&mut source, &mut text, &mut scene);
    if options.cache_stats {
        surface.draw_scene(&scene, peniko::Color::WHITE).unwrap();
    }
    let mut samples = Vec::with_capacity(options.updates);
    let (mut declared, mut backend, mut visited, mut flattened, mut layouts) =
        (0_u128, 0_u128, 0, 0, 0);
    for i in 0..options.warmup + options.updates {
        let start = Instant::now();
        source.view.set(i + 1);
        source.request_rebuild();
        prepare(&mut source, &mut text, &mut scene);
        if i >= options.warmup {
            samples.push(start.elapsed().as_nanos());
            declared += source.declaration_micros;
            backend += source.adapter.stats.layout_micros;
            visited += source.declaration_stats.visited;
            flattened += source.adapter.stats.flatten_visits;
            layouts += source.adapter.stats.layout_runs;
        }
    }
    if options.cache_stats {
        surface.draw_scene(&scene, peniko::Color::WHITE).unwrap();
    }
    if !immediate {
        assert!(
            source.adapter.stats.scene_fragment_builds + source.adapter.stats.scene_fragment_reuses
                > 0,
            "reactive benchmark must exercise retained Scene fragments"
        );
    }
    samples.sort_unstable();
    let n = options.updates as f64;
    println!(
        "{},{:.2},{:.2},{:.2},{:.2},{:.1},{:.1},{:.1},{},{}",
        if immediate { "immediate" } else { "reactive" },
        samples[options.updates / 2] as f64 / 1000.,
        samples[(options.updates * 95 / 100).min(options.updates - 1)] as f64 / 1000.,
        declared as f64 / n,
        backend as f64 / n,
        visited as f64 / n,
        flattened as f64 / n,
        layouts as f64 / n,
        scene.commands.len(),
        scene.flattened().commands.len()
    );
    let result: Vec<_> = source
        .adapter
        .placed
        .iter()
        .filter_map(|p| {
            p.paint
                .text
                .as_ref()
                .filter(|t| t.len() == 6 && t.as_bytes()[3] == b':')
                .map(|t| (t.to_string(), p.rect))
        })
        .collect();
    if !options.downloads {
        assert_eq!(result.len(), 500);
    }
    if options.downloads {
        let expected = format!("{}%", (options.warmup + options.updates) % 100);
        assert!(
            source
                .adapter
                .placed
                .iter()
                .any(|p| p.paint.text.as_deref() == Some(expected.as_str())),
            "active task must reach the expected progress"
        );
        assert!(
            source.adapter.placed.iter().all(|p| p.rect[0] >= 0.
                && p.rect[1] >= 0.
                && p.rect[2] <= 1440.1
                && p.rect[3] <= 1060.1),
            "all dashboard nodes must fit in the viewport"
        );
        return source
            .adapter
            .placed
            .iter()
            .map(|p| {
                (
                    format!(
                        "{:?}|{:?}|{}",
                        p.paint.text, p.paint.background, p.paint.opacity
                    ),
                    p.rect,
                )
            })
            .collect();
    }
    assert!(
        result
            .iter()
            .all(|(_, r)| r[0] >= 0. && r[1] >= 0. && r[2] <= 1440. && r[3] <= 1060.)
    );
    result
}
fn main() -> Result<(), Error> {
    let options = Options::parse();
    if std::env::var_os("PLICO_PHASE_STATS").is_some()
        && (!cfg!(feature = "profiling") || options.cache_stats || options.headless)
    {
        return Err(Error::Option {
            message: "PLICO_PHASE_STATS requires profiling, a window, and no --cache-stats",
        });
    }
    if options.cache_stats {
        tracing_subscriber::fmt()
            .with_env_filter("plico::cache=trace")
            .with_writer(std::io::stderr)
            .init();
    }
    #[cfg(feature = "profiling")]
    if std::env::var_os("PLICO_PHASE_STATS").is_some() {
        visible_texts_profile::install();
    }
    let _trace_guard = (!options.cache_stats
        && std::env::var_os("PLICO_PHASE_STATS").is_none()
        && std::env::var_os("PLICO_TRACE_FILE").is_some())
    .then(plico_shell::telemetry::init_tracing);
    if !options.cache_stats
        && std::env::var_os("PLICO_PHASE_STATS").is_none()
        && _trace_guard.is_none()
        && std::env::var_os("RUST_LOG").is_some()
    {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();
    }
    if options.hz == 0 || options.hz > 240 || options.updates == 0 {
        return Err(Error::Option {
            message: "hz must be 1..240 and updates must be positive",
        });
    }
    if options.headless {
        println!(
            "model,p50_us,p95_us,mean_declare_us,mean_backend_us,visited,flattened,layout_runs,top_level_commands,expanded_commands"
        );
        let a = bench(true, &options);
        let b = bench(false, &options);
        assert_eq!(a, b, "both models must produce identical text and geometry");
        return Ok(());
    }
    let mut view = Texts::new(options.immediate, options.downloads);
    view.feed = Some(Feed::new(options.hz));
    plico_shell::run_window_with_options(
        Box::new(Source::new(view)),
        WindowOptions::new()
            .title(match (options.downloads, options.immediate) {
                (true, true) => "Plico downloads / immediate",
                (true, false) => "Plico downloads / reactive",
                (false, true) => "Plico visible texts / immediate",
                (false, false) => "Plico visible texts / reactive",
            })
            .initial_inner_size(1440, 1060)
            .min_inner_size(1180, 1000)
            .renderer(RenderBackend::Skia),
    )
    .map_err(|source| Error::Window { source })
}
