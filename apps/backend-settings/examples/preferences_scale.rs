//! Full native form pipeline. No Scene, window or GPU. Run with --release.
use backend_settings::preferences_regions::scale::{
    FullRegionPanels, ImmediatePanels, region_panels,
};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext, UiEvent};
use plico_runtime::{Source, View};
use std::{
    hash::{Hash, Hasher},
    hint::black_box,
    time::Instant,
};
#[derive(usage_rs::Cli)]
#[usage(about = "Scale the real Preferences forms and batched native interactions")]
struct Options {
    #[usage(long, default = "1,8,32,128,512")]
    sizes: String,
    /// Add the same region tree with local replay disabled (bookkeeping remains).
    #[usage(long)]
    full_control: bool,
    /// Counts or percentages of forms changed per frame, e.g. 1,10%,100%.
    #[usage(long, default = "1,10%,100%")]
    updates: String,
    #[usage(long, default = "notifications,typing,save,advanced,theme")]
    scenarios: String,
    #[usage(long, default = "100")]
    iterations: usize,
    #[usage(long, default = "20")]
    warmup: usize,
    /// Zero uses 780px per form so all controls are materialized and visible.
    #[usage(long, default = "0")]
    viewport_height: u32,
}
#[derive(Debug, snafu::Snafu)]
enum Error {
    #[snafu(display("invalid benchmark option: {message}"))]
    Option { message: String },
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Option {
        message: message.into(),
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Notifications,
    Typing,
    Save,
    Advanced,
    Theme,
}
impl Scenario {
    fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "notifications" => Ok(Self::Notifications),
            "typing" => Ok(Self::Typing),
            "save" => Ok(Self::Save),
            "advanced" => Ok(Self::Advanced),
            "theme" => Ok(Self::Theme),
            _ => Err(invalid(format!("unknown scenario {s:?}"))),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Notifications => "notifications",
            Self::Typing => "typing",
            Self::Save => "save",
            Self::Advanced => "advanced",
            Self::Theme => "theme",
        }
    }
    fn control(self) -> (SemanticRole, &'static str) {
        match self {
            Self::Notifications => (SemanticRole::CheckBox, "Enable notifications"),
            Self::Typing => (SemanticRole::TextInput, "Workspace name"),
            Self::Save => (SemanticRole::Button, "Save changes"),
            Self::Advanced => (SemanticRole::CheckBox, "Show advanced options"),
            Self::Theme => (SemanticRole::CheckBox, "Dark theme"),
        }
    }
}
fn prepare<V: View>(source: &mut Source<V>, text: &mut TextContext, height: u32) {
    source.experiment_prepare_without_scene(text, 900, height);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn ids<V: View>(source: &Source<V>, scenario: Scenario, k: usize) -> Vec<u64> {
    let (role, label) = scenario.control();
    let tree = source.adapter.semantics(&source.engine, 0);
    let ids: Vec<_> = tree
        .exposed_nodes()
        .into_iter()
        .filter(|n| n.role == role && n.label.as_deref() == Some(label))
        .map(|n| n.id)
        .take(k)
        .collect();
    assert_eq!(ids.len(), k, "not enough accessible controls");
    ids
}
fn event<V: View>(source: &mut Source<V>, scenario: Scenario, targets: &[u64], index: usize) {
    for &id in targets {
        if scenario == Scenario::Typing {
            assert!(source.handle_semantic_action(SemanticAction::Focus { target: id }));
            let result = if index.is_multiple_of(2) {
                source.handle_event(UiEvent::TextInput("x".into()))
            } else {
                source.handle_event(UiEvent::Key(plico::KeyEvent {
                    phase: plico::KeyPhase::Down,
                    key: "Backspace".into(),
                    code: "Backspace".into(),
                    key_without_modifiers: "Backspace".into(),
                    text: None,
                    text_with_all_modifiers: None,
                    location: plico::KeyLocation::Standard,
                    modifiers: plico::Modifiers::empty(),
                    repeat: false,
                    synthetic: false,
                }))
            };
            assert!(result.handled);
        } else {
            assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Evidence {
    first: u64,
    last: u64,
}
fn fingerprint<V: View>(source: &Source<V>) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let tree = source.adapter.semantics(&source.engine, 0);
    for node in tree.exposed_nodes().into_iter().filter(|n| {
        matches!(
            n.role,
            SemanticRole::Label
                | SemanticRole::Heading
                | SemanticRole::Button
                | SemanticRole::TextInput
                | SemanticRole::CheckBox
        )
    }) {
        // IDs and extra container topology intentionally differ between APIs.
        format!(
            "{:?}",
            (
                node.role,
                &node.label,
                &node.value,
                node.disabled,
                node.states
            )
        )
        .hash(&mut hash);
    }
    hash.finish()
}
fn bench<V: View>(
    mode: &str,
    view: V,
    panels: usize,
    k: usize,
    scenario: Scenario,
    height: u32,
    options: &Options,
) -> Evidence {
    let mut source = Source::new(view);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    prepare(&mut source, &mut text, height);
    let targets = ids(&source, scenario, k);
    let mut samples = Vec::with_capacity(options.iterations);
    let mut first = None;
    let mut auxiliary = [0_usize; 7];
    let (mut event_ns, mut declare_us, mut sync_us, mut layout_us) =
        (0_u128, 0_u128, 0_u128, 0_u128);
    let (mut flattened, mut layout_runs, mut sync_visits, mut passes) =
        (0_usize, 0_usize, 0_usize, 0_usize);
    for index in 0..options.warmup + options.iterations {
        let before = source.batch_stats().declaration_passes;
        let start = Instant::now();
        event(&mut source, scenario, &targets, index);
        let events = start.elapsed().as_nanos();
        prepare(&mut source, &mut text, height);
        let elapsed = start.elapsed().as_nanos();
        black_box(&source.adapter.placed);
        if index == options.warmup {
            first = Some(fingerprint(&source));
        }
        if index >= options.warmup {
            samples.push(elapsed);
            event_ns += events;
            declare_us += source.declaration_micros;
            sync_us += source.adapter.stats.sync_micros;
            layout_us += source.adapter.stats.layout_micros;
            flattened += source.adapter.stats.flatten_visits;
            layout_runs += source.adapter.stats.layout_runs;
            sync_visits += source.adapter.stats.sync_visits;
            passes += source.batch_stats().declaration_passes - before;
            for (sum, value) in auxiliary.iter_mut().zip([
                source.adapter.stats.selection_visits,
                source.adapter.stats.slider_visits,
                source.adapter.stats.shader_visits,
                source.adapter.stats.input_rebuilds,
                source.adapter.stats.input_reuses,
                source.adapter.stats.scroll_visits,
                source.adapter.stats.tooltip_visits,
            ]) {
                *sum += value;
            }
        }
    }
    samples.sort_unstable();
    let n = options.iterations as f64;
    println!(
        "{mode},{panels},{k},{:.5},{},{height},{},{},{:.0},{:.2},{:.2},{:.2},{:.1},{:.1},{:.1},{:.1},{},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}",
        k as f64 / panels as f64,
        scenario.name(),
        samples[options.iterations / 2],
        samples[(options.iterations * 95 / 100).min(options.iterations - 1)],
        event_ns as f64 / n,
        declare_us as f64 / n,
        sync_us as f64 / n,
        layout_us as f64 / n,
        flattened as f64 / n,
        layout_runs as f64 / n,
        sync_visits as f64 / n,
        passes as f64 / n,
        source.experiment_live_counts().0,
        auxiliary[0] as f64 / n,
        auxiliary[1] as f64 / n,
        auxiliary[2] as f64 / n,
        auxiliary[3] as f64 / n,
        auxiliary[4] as f64 / n,
        auxiliary[5] as f64 / n,
        auxiliary[6] as f64 / n
    );
    Evidence {
        first: first.unwrap(),
        last: fingerprint(&source),
    }
}
fn run(options: Options) -> Result<(), Error> {
    if options.iterations == 0 {
        return Err(invalid("iterations must be positive"));
    }
    options
        .warmup
        .checked_add(options.iterations)
        .ok_or_else(|| invalid("iteration count overflow"))?;
    let sizes: Vec<usize> = options
        .sizes
        .split(',')
        .map(|v| {
            v.trim()
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid(format!("invalid size {v:?}")))
        })
        .collect::<Result<_, _>>()?;
    let scenarios: Vec<_> = options
        .scenarios
        .split(',')
        .map(|v| Scenario::parse(v.trim()))
        .collect::<Result<_, _>>()?;
    println!(
        "mode,panels,target_panels,target_fraction,scenario,viewport_height,p50_total_ns,p95_total_ns,mean_event_ns,mean_declare_us,mean_sync_us,mean_backend_update_us,mean_flatten_visits,mean_layout_runs,mean_sync_visits,mean_declaration_passes,nodes,mean_selection_visits,mean_slider_visits,mean_shader_visits,mean_input_rebuilds,mean_input_reuses,mean_scroll_visits,mean_tooltip_visits"
    );
    for panels in sizes {
        let height = if options.viewport_height > 0 {
            options.viewport_height
        } else {
            u32::try_from(panels)
                .ok()
                .and_then(|n| n.checked_mul(780))
                .ok_or_else(|| invalid("viewport height overflow"))?
        };
        let mut updates = vec![];
        for value in options.updates.split(',').map(str::trim) {
            let k = if let Some(percent) = value.strip_suffix('%') {
                let percent = percent
                    .parse::<usize>()
                    .ok()
                    .filter(|p| *p > 0 && *p <= 100)
                    .ok_or_else(|| invalid(format!("invalid percentage {value:?}")))?;
                panels
                    .checked_mul(percent)
                    .ok_or_else(|| invalid("update count overflow"))?
                    .div_ceil(100)
            } else {
                value
                    .parse::<usize>()
                    .ok()
                    .filter(|k| *k > 0)
                    .ok_or_else(|| invalid(format!("invalid update count {value:?}")))?
            };
            if k <= panels && !updates.contains(&k) {
                updates.push(k);
            }
        }
        for scenario in &scenarios {
            // Theme is global; repeating clicks on mirrored toggles is not K
            // independent state mutations. Measure a single broadcast instead.
            let counts = if *scenario == Scenario::Theme {
                vec![1]
            } else {
                updates.clone()
            };
            for k in counts {
                let full = options.full_control.then(|| {
                    bench(
                        "regions_full",
                        FullRegionPanels::new(panels),
                        panels,
                        k,
                        *scenario,
                        height,
                        &options,
                    )
                });
                // Alternate measurement order to reduce systematic warm-order bias.
                let (immediate, regions) = if (panels + k).is_multiple_of(2) {
                    let regions = bench(
                        "regions",
                        region_panels(panels),
                        panels,
                        k,
                        *scenario,
                        height,
                        &options,
                    );
                    let immediate = bench(
                        "immediate",
                        ImmediatePanels::new(panels),
                        panels,
                        k,
                        *scenario,
                        height,
                        &options,
                    );
                    (immediate, regions)
                } else {
                    let immediate = bench(
                        "immediate",
                        ImmediatePanels::new(panels),
                        panels,
                        k,
                        *scenario,
                        height,
                        &options,
                    );
                    let regions = bench(
                        "regions",
                        region_panels(panels),
                        panels,
                        k,
                        *scenario,
                        height,
                        &options,
                    );
                    (immediate, regions)
                };
                assert_eq!(
                    immediate,
                    regions,
                    "different final UI: panels={panels},k={k},scenario={}",
                    scenario.name()
                );
                if let Some(full) = full {
                    assert_eq!(full, regions, "full/local region trees disagree");
                }
            }
        }
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run(Options::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
