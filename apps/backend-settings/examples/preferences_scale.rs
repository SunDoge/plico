//! Native form pipeline; optional Scene encoding and Skia CPU raster, no window/GPU.
#[cfg(feature = "profiling")]
mod support;
use backend_settings::preferences_regions::scale::{
    FullRegionPanels, ImmediatePanels, region_panels,
};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext, UiEvent};
use plico_runtime::{HostView, Source};
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
    /// Force full flattening while retaining the same local Reactive replay.
    #[usage(long)]
    full_geometry: bool,
    /// Include backend-neutral Scene encoding in every measured frame.
    #[usage(long)]
    scene: bool,
    /// Include Scene encoding and full Skia CPU rasterization (requires skia).
    #[usage(long)]
    raster: bool,
    /// Track Scene damage and skip or clip Skia CPU rasterization (requires skia).
    #[usage(long)]
    tracked_raster: bool,
    /// Force full Scene encoding while retaining local Reactive replay.
    #[usage(long)]
    full_scene: bool,
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
fn prepare<V: HostView>(source: &mut Source<V>, text: &mut TextContext, height: u32) {
    source.experiment_prepare_without_scene(text, 900, height);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn ids<V: HostView>(source: &Source<V>, scenario: Scenario, k: usize) -> Vec<u64> {
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
fn event<V: HostView>(source: &mut Source<V>, scenario: Scenario, targets: &[u64], index: usize) {
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
    pixels: Option<u64>,
}
fn fingerprint<V: HostView>(source: &Source<V>) -> u64 {
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
#[cfg(feature = "skia")]
fn tracked_pixel_delta(actual: &[u8], expected: &[u8]) -> (usize, u8) {
    let differing = actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    let max_delta = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    (differing, max_delta)
}

fn bench<V: HostView>(
    mode: &str,
    view: V,
    panels: usize,
    k: usize,
    scenario: Scenario,
    height: u32,
    options: &Options,
) -> Evidence {
    let mut source = Source::new(view);
    source.adapter.experiment_force_full_geometry = options.full_geometry;
    source.experiment_force_full_scene = options.full_scene;
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    #[cfg(feature = "skia")]
    let mut raster = (options.raster || options.tracked_raster)
        .then(|| plico_skia::SkiaSurface::new(900, height).unwrap());
    #[cfg(not(feature = "skia"))]
    assert!(
        !options.raster && !options.tracked_raster,
        "--raster and --tracked-raster require --features skia"
    );
    prepare(&mut source, &mut text, height);
    let targets = ids(&source, scenario, k);
    #[cfg(feature = "profiling")]
    let mut phases = [0_u128; 8];
    let mut samples = Vec::with_capacity(options.iterations);
    let mut first = None;
    let mut auxiliary = [0_usize; 7];
    let (mut event_ns, mut declare_us, mut sync_us, mut layout_us) =
        (0_u128, 0_u128, 0_u128, 0_u128);
    let (mut flattened, mut layout_runs, mut sync_visits, mut passes) =
        (0_usize, 0_usize, 0_usize, 0_usize);
    let (mut scene_us, mut scene_builds, mut flatten_patches, mut layout_checks) =
        (0_u128, 0_usize, 0_usize, 0_usize);
    let (mut fragment_builds, mut fragment_reuses, mut scene_nodes) = (0_usize, 0_usize, 0_usize);
    let (mut scene_full, mut scene_invalid) = (0_usize, 0_usize);
    #[cfg_attr(not(feature = "skia"), allow(unused_mut))]
    let mut raster_us = 0_u128;
    #[cfg_attr(not(feature = "skia"), allow(unused_mut))]
    let (mut damage_coverage, mut damage_empty, mut damage_partial, mut damage_full) =
        (0_f64, 0_usize, 0_usize, 0_usize);
    for index in 0..options.warmup + options.iterations {
        let before = source.batch_stats().declaration_passes;
        #[cfg(feature = "profiling")]
        support::reset();
        let start = Instant::now();
        event(&mut source, scenario, &targets, index);
        let events = start.elapsed().as_nanos();
        if options.scene || options.raster || options.tracked_raster {
            let timing = source
                .render_retained_frame(&mut scene, &mut text, (900, height), 1.)
                .unwrap();
            if index >= options.warmup {
                scene_us += (timing.2 * 1000.) as u128;
            }
            black_box(&scene.commands);
        } else {
            prepare(&mut source, &mut text, height);
        }
        #[cfg(feature = "skia")]
        if let Some(surface) = &mut raster {
            let start = Instant::now();
            let damage = if options.tracked_raster {
                surface
                    .draw_scene_tracked(&scene, source.base_color())
                    .unwrap()
            } else {
                surface.draw_scene(&scene, source.base_color()).unwrap();
                plico_renderer::DamageRegion::Full
            };
            if index >= options.warmup {
                raster_us += start.elapsed().as_nanos();
                if options.tracked_raster {
                    damage_coverage += damage.coverage(900, height);
                    match damage {
                        plico_renderer::DamageRegion::Empty => damage_empty += 1,
                        plico_renderer::DamageRegion::Rect(_) => damage_partial += 1,
                        plico_renderer::DamageRegion::Full => damage_full += 1,
                    }
                }
            }
        }
        assert!(source.last_error.is_none(), "{:?}", source.last_error);
        let elapsed = start.elapsed().as_nanos();
        black_box(&source.adapter.placed);
        if index == options.warmup {
            first = Some(fingerprint(&source));
        }
        if index >= options.warmup {
            #[cfg(feature = "profiling")]
            support::accumulate(&mut phases);
            samples.push(elapsed);
            event_ns += events;
            declare_us += source.declaration_micros;
            sync_us += source.adapter.stats.sync_micros;
            layout_us += source.adapter.stats.layout_micros;
            flattened += source.adapter.stats.flatten_visits;
            layout_runs += source.adapter.stats.layout_runs;
            sync_visits += source.adapter.stats.sync_visits;
            passes += source.batch_stats().declaration_passes - before;
            scene_builds += source.adapter.stats.scene_builds;
            flatten_patches += source.adapter.stats.flatten_patches;
            layout_checks += source.adapter.stats.layout_cache_checks;
            fragment_builds += source.adapter.stats.scene_fragment_builds;
            fragment_reuses += source.adapter.stats.scene_fragment_reuses;
            scene_nodes += source.adapter.stats.scene_encoded_nodes;
            scene_full += source.adapter.stats.scene_full_rebuilds;
            scene_invalid += source.adapter.stats.scene_invalid_roots;
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
    #[cfg_attr(not(feature = "skia"), allow(unused_mut))]
    let (mut tracked_pixel_differences, mut tracked_max_channel_delta) = (0_usize, 0_u8);
    let pixels = {
        #[cfg(feature = "skia")]
        {
            raster.as_mut().map(|surface| {
                let mut pixels = surface.read_pixels().unwrap();
                if options.tracked_raster {
                    surface.draw_scene(&scene, source.base_color()).unwrap();
                    let full = surface.read_pixels().unwrap();
                    (tracked_pixel_differences, tracked_max_channel_delta) =
                        tracked_pixel_delta(&pixels, &full);
                    pixels = full;
                }
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                pixels.hash(&mut hash);
                hash.finish()
            })
        }
        #[cfg(not(feature = "skia"))]
        {
            None
        }
    };
    let pixel_fingerprint = pixels
        .map(|hash| format!("{hash:016x}"))
        .unwrap_or_default();
    println!(
        "{mode},{panels},{k},{:.5},{},{height},{},{},{:.0},{:.2},{:.2},{:.2},{:.1},{:.1},{:.1},{:.1},{},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.2},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.2},{:.5},{:.1},{:.1},{:.1},{tracked_pixel_differences},{tracked_max_channel_delta},{pixel_fingerprint}",
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
        auxiliary[6] as f64 / n,
        scene_us as f64 / n,
        scene_builds as f64 / n,
        flatten_patches as f64 / n,
        layout_checks as f64 / n,
        fragment_builds as f64 / n,
        fragment_reuses as f64 / n,
        scene_nodes as f64 / n,
        scene_full as f64 / n,
        scene_invalid as f64 / n,
        raster_us as f64 / n / 1000.,
        damage_coverage / n,
        damage_empty as f64 / n,
        damage_partial as f64 / n,
        damage_full as f64 / n,
    );
    #[cfg(feature = "profiling")]
    support::report(
        &format!(
            "mode={mode} panels={panels} k={k} scenario={}",
            scenario.name()
        ),
        phases,
        options.iterations,
    );
    Evidence {
        first: first.unwrap(),
        last: fingerprint(&source),
        pixels,
    }
}
fn run(options: Options) -> Result<(), Error> {
    if options.iterations == 0 {
        return Err(invalid("iterations must be positive"));
    }
    if options.raster && options.tracked_raster {
        return Err(invalid(
            "--raster and --tracked-raster are mutually exclusive",
        ));
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
        "mode,panels,target_panels,target_fraction,scenario,viewport_height,p50_total_ns,p95_total_ns,mean_event_ns,mean_declare_us,mean_sync_us,mean_backend_update_us,mean_flatten_visits,mean_layout_runs,mean_sync_visits,mean_declaration_passes,nodes,mean_selection_visits,mean_slider_visits,mean_shader_visits,mean_input_rebuilds,mean_input_reuses,mean_scroll_visits,mean_tooltip_visits,mean_scene_us,mean_scene_builds,mean_flatten_patches,mean_layout_cache_checks,mean_scene_fragment_builds,mean_scene_fragment_reuses,mean_scene_encoded_nodes,mean_scene_full_rebuilds,mean_scene_invalid_roots,mean_raster_us,mean_damage_coverage,empty_frame_fraction,partial_frame_fraction,full_frame_fraction,tracked_pixel_differences,tracked_max_channel_delta,pixel_fingerprint"
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
    #[cfg(feature = "profiling")]
    support::install();
    match run(Options::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
