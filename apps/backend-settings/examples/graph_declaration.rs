//! Sparse activity in a large workflow graph, with viewport culling as a control.
mod bench_support;
use bench_support::{Feed, thread_cpu_ns};
use plico::{Engine, Entity, FrameError, ViewNode, prelude::*};
use plico_renderer::{RenderBackend, WakeCallback, WindowOptions};
use plico_runtime::{
    Source, View,
    regions::{RegionUi, Regions, ReplayBatch, State},
};
use std::{sync::atomic::Ordering, time::Instant};

#[derive(usage_rs::Cli)]
#[usage(about = "Workflow graph: full declaration, viewport culling, or local region replay")]
struct Options {
    #[usage(long, default = "512")]
    nodes: usize,
    #[usage(long, default = "12")]
    ports: usize,
    #[usage(long)]
    immediate: bool,
    #[usage(long)]
    visible_only: bool,
    /// Measure declaration + reconciliation only, with no layout or renderer.
    #[usage(long)]
    headless: bool,
    #[usage(long, default = "60")]
    hz: usize,
    #[usage(long, default = "120")]
    updates: usize,
    #[usage(long, default = "40")]
    warmup: usize,
}
#[derive(Debug, snafu::Snafu)]
enum Error {
    #[snafu(display("invalid benchmark dimensions or update count"))]
    Options,
    #[snafu(display("window failed: {source}"))]
    Window { source: plico_shell::Error },
}
#[derive(Clone, Copy)]
struct Config {
    nodes: usize,
    ports: usize,
    visible_only: bool,
}
#[derive(Clone, Copy)]
struct NodeInfo {
    index: usize,
    ports: usize,
}
struct GraphState {
    config: Config,
    info: Vec<State<NodeInfo>>,
    tick: State<usize>,
}
struct Graph {
    config: Config,
    tick: usize,
    reactive: Option<(Regions, State<usize>)>,
    feed: Option<Feed>,
}
impl Graph {
    fn new(config: Config, immediate: bool) -> Self {
        let reactive = if immediate {
            None
        } else {
            let mut handle = None;
            let regions = Regions::new(|app| {
                let tick = app.state(|| 0_usize);
                handle = Some(tick);
                let info = (0..config.nodes)
                    .map(|index| {
                        app.state(|| NodeInfo {
                            index,
                            ports: config.ports,
                        })
                    })
                    .collect();
                let graph = app.state(|| GraphState { config, info, tick });
                app.root(graph, |cx, graph| {
                    let graph = graph.read();
                    canvas(cx, graph.config, 0, Some(&graph));
                });
            });
            Some((regions, handle.unwrap()))
        };
        Self {
            config,
            tick: 0,
            reactive,
            feed: None,
        }
    }
    fn set(&mut self, tick: usize) {
        self.tick = tick;
        if let Some((regions, state)) = &self.reactive {
            regions.access(*state).set(tick);
        }
    }
}
const WIDTH: f32 = 1440.;
const HEIGHT: f32 = 1000.;
const CANVAS_HEIGHT: f32 = 920.;
fn position(index: usize) -> [f32; 2] {
    [
        (index % 32) as f32 * 240. + 12.,
        (index / 32) as f32 * 290. + 12.,
    ]
}
fn visible(region: VisibleRegion, index: usize, ports: usize) -> bool {
    let [x, y] = position(index);
    region.intersects([x, y, x + 220., y + ports as f32 * 18. + 64.])
}
#[component]
fn canvas(cx: &mut UiContext<'_>, config: Config, tick: usize, reactive: Option<&GraphState>) {
    let viewport =
        VisibleRegion::from_size([cx.viewport_width(), (cx.viewport_size()[1] - 80.).max(0.)]);
    column(cx).width_percent(100.).height_percent(100.).background(Color::Surface).children(|cx| {
        column(cx).height(80.).padding(12).gap(4).shrink(0.).children(|cx| {
            text(cx, "Workflow graph · sparse execution activity").font_size(22).weight(700);
            text(cx, "Simulated graph, one active node. Offscreen nodes are clipped. --visible-only also culls declarations.").font_size(13);
        });
        column(cx).width_percent(100.).height(0.).grow(1.).clip(true).background(Color::Panel).children(|cx| {
            for index in 0..config.nodes {
                if config.visible_only && !visible(viewport, index, config.ports) { continue; }
                let [x,y] = position(index);
                column(cx).key(index as u64).absolute(x,y).width(220.).height(config.ports as f32 * 18. + 64.)
                    .padding(8).radius(8.).background(Color::Surface).border(1.,Color::Border).children(|cx| {
                        if let Some(graph) = reactive {
                            if index == 0 {
                                cx.region_keyed(index as u64, (graph.info[index], graph.tick), |cx, (info,tick)| {
                                    node_card(cx, *info.read(), *tick.read());
                                });
                            } else {
                                cx.region_keyed(index as u64, graph.info[index], |cx, info| node_card(cx, *info.read(), 0));
                            }
                        } else {
                            column(cx).children(|cx| node_card(cx, NodeInfo {index,ports: config.ports}, if index == 0 {tick} else {0}));
                        }
                    });
            }
        });
    });
}
#[component]
fn node_card(cx: &mut UiContext<'_>, info: NodeInfo, tick: usize) {
    const KINDS: [&str; 6] = [
        "Sampler",
        "Load image",
        "Text encoder",
        "VAE decode",
        "Upscale",
        "Save image",
    ];
    const PORTS: [&str; 6] = ["model", "image", "mask", "prompt", "latent", "conditioning"];
    let active = info.index == 0;
    let phase = tick % 120;
    let pulse = if phase < 60 { phase } else { 120 - phase };
    let activity = if active {
        Color::Literal(0x5580a0 + ((pulse as u32 * 2) << 16))
    } else {
        Color::Border
    };
    column(cx).width_percent(100.).gap(0).children(|cx| {
        row(cx).height(24.).align(Align::Center).children(|cx| {
            text(cx, KINDS[info.index % KINDS.len()])
                .font_size(14)
                .weight(650)
                .grow(1.);
            text(cx, if active { "Running" } else { "Ready" })
                .font_size(13)
                .color(if active { Color::Accent } else { Color::Muted });
        });
        for port in 0..info.ports {
            row(cx)
                .key(port as u64)
                .height(18.)
                .gap(8)
                .align(Align::Center)
                .children(|cx| {
                    column(cx)
                        .width(6.)
                        .height(6.)
                        .radius(3.)
                        .background(activity)
                        .end();
                    text(cx, PORTS[port % PORTS.len()]).font_size(13).grow(1.);
                    text(
                        cx,
                        if port.is_multiple_of(2) {
                            "INPUT"
                        } else {
                            "OUTPUT"
                        },
                    )
                    .font_size(13)
                    .color(Color::Muted);
                });
        }
        row(cx).height(22.).gap(6).children(|cx| {
            button(cx)
                .text("Inspect")
                .ghost()
                .height(20.)
                .padding(0)
                .font_size(13);
            button(cx)
                .text("Bypass")
                .ghost()
                .height(20.)
                .padding(0)
                .font_size(13);
        });
    });
}
impl View for Graph {
    fn theme(&self) -> Theme {
        Theme::default()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        if let Some((regions, _)) = &mut self.reactive {
            regions.show(cx);
        } else {
            column(cx)
                .key("regions-root")
                .width_percent(100.)
                .height_percent(100.)
                .children(|cx| canvas(cx, self.config, self.tick, None));
        }
    }
    fn replay_regions(&mut self, engine: &mut Engine) -> Option<Result<ReplayBatch, FrameError>> {
        self.reactive
            .as_mut()
            .and_then(|(r, _)| r.replay_regions(engine))
    }
    fn scene_cache_roots(&self) -> Vec<Entity> {
        self.reactive
            .as_ref()
            .map_or_else(Vec::new, |(r, _)| r.scene_cache_roots())
    }
    fn snapshot_changed(&self) -> bool {
        self.feed
            .as_ref()
            .is_some_and(|f| f.value.load(Ordering::Acquire) != self.tick)
            || self
                .reactive
                .as_ref()
                .is_some_and(|(r, _)| r.snapshot_changed())
    }
    fn poll_reload(&mut self) -> bool {
        let tick = self
            .feed
            .as_ref()
            .map_or(self.tick, |f| f.value.load(Ordering::Acquire));
        if tick == self.tick {
            false
        } else {
            self.set(tick);
            true
        }
    }
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        if let Some(f) = &self.feed {
            *f.wake.lock().unwrap() = Some(wake);
        }
    }
}
fn visible_snapshot(nodes: &[ViewNode], output: &mut Vec<String>) {
    for node in nodes {
        if let Some([x, y]) = node.style.absolute
            && (x >= WIDTH || y >= CANVAS_HEIGHT)
        {
            continue;
        }
        output.push(format!("{:?}|{:?}|{}", node.kind, node.style, node.text));
        visible_snapshot(&node.children, output);
    }
}
fn bench(config: Config, immediate: bool, options: &Options) -> Vec<String> {
    let mut source = Source::new(Graph::new(config, immediate));
    source.engine.set_viewport(WIDTH, HEIGHT).unwrap();
    source.try_declare().unwrap();
    let mut samples = Vec::with_capacity(options.updates);
    let mut visited = 0;
    let mut cpu_start = 0;
    for i in 0..options.warmup + options.updates {
        if i == options.warmup {
            cpu_start = thread_cpu_ns();
        }
        let start = Instant::now();
        source.view.set(i + 1);
        source.try_declare().unwrap();
        if i >= options.warmup {
            samples.push(start.elapsed().as_nanos());
            visited += source.declaration_stats.visited;
        }
    }
    let cpu_us = if cfg!(target_os = "linux") {
        format!(
            "{:.2}",
            (thread_cpu_ns() - cpu_start) as f64 / options.updates as f64 / 1000.
        )
    } else {
        "NA".to_owned()
    };
    samples.sort_unstable();
    println!(
        "{},{},{},{:.2},{:.2},{},{:.1}",
        if immediate { "immediate" } else { "region" },
        config.visible_only,
        config.nodes,
        samples[options.updates / 2] as f64 / 1000.,
        samples[(options.updates * 95 / 100).min(options.updates - 1)] as f64 / 1000.,
        cpu_us,
        visited as f64 / options.updates as f64
    );
    let mut output = Vec::new();
    visible_snapshot(&source.engine.snapshot(), &mut output);
    output
}
fn main() -> Result<(), Error> {
    let options = Options::parse();
    if options.nodes == 0
        || options.nodes > 2048
        || options.ports == 0
        || options.ports > 24
        || options.hz == 0
        || options.hz > 240
        || options.updates == 0
    {
        return Err(Error::Options);
    }
    let config = Config {
        nodes: options.nodes,
        ports: options.ports,
        visible_only: options.visible_only,
    };
    if options.headless {
        println!(
            "model,visible_only,graph_nodes,p50_declare_diff_us,p95_declare_diff_us,mean_thread_cpu_us,visited"
        );
        let reference = bench(
            Config {
                visible_only: false,
                ..config
            },
            true,
            &options,
        );
        for (immediate, visible_only) in [(true, true), (false, false), (false, true)] {
            let output = bench(
                Config {
                    visible_only,
                    ..config
                },
                immediate,
                &options,
            );
            assert_eq!(
                reference, output,
                "visible declarations differ: immediate={immediate}, visible_only={visible_only}"
            );
        }
        return Ok(());
    }
    let mut view = Graph::new(config, options.immediate);
    view.feed = Some(Feed::new(options.hz));
    plico_shell::run_window_with_options(
        Box::new(Source::new(view)),
        WindowOptions::new()
            .title("Plico workflow graph workload")
            .initial_inner_size(WIDTH as u32, HEIGHT as u32)
            .min_inner_size(1000, 700)
            .renderer(RenderBackend::Skia),
    )
    .map_err(|source| Error::Window { source })
}
