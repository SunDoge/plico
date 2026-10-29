//! Complete routed application CPU pipeline, without a window, Scene or GPU.
//! Run in release mode; both implementations execute the same semantic events.
#[cfg(feature = "profiling")]
mod support;
use backend_settings::{ImmediateSettings, Settings};
use plico::{KeyEvent, KeyLocation, KeyPhase, Modifiers, VirtualClock};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext, UiEvent};
use plico_runtime::{Source, View};
use std::{
    hash::{Hash, Hasher},
    hint::black_box,
    time::{Duration, Instant},
};

#[derive(usage_rs::Cli)]
#[usage(about = "Profile the complete routed settings application (no GPU)")]
struct Options {
    #[usage(long, default = "2000")]
    iterations: usize,
    #[usage(long, default = "200")]
    warmup: usize,
    #[usage(
        long,
        default = "idle,route,reverse,notifications,typing,save,advanced,theme,details_motion"
    )]
    scenarios: String,
}

trait BenchView: View + Default {
    fn reduce_motion(&mut self);
}
impl BenchView for ImmediateSettings {
    fn reduce_motion(&mut self) {
        self.reduced_motion = true;
    }
}
impl BenchView for Settings {
    fn reduce_motion(&mut self) {
        self.set_reduced_motion(true);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Idle,
    Route,
    Reverse,
    Notifications,
    Typing,
    Save,
    Advanced,
    Theme,
    DetailsMotion,
}
impl Scenario {
    fn parse(value: &str) -> Self {
        match value {
            "idle" => Self::Idle,
            "route" => Self::Route,
            "reverse" => Self::Reverse,
            "notifications" => Self::Notifications,
            "typing" => Self::Typing,
            "save" => Self::Save,
            "advanced" => Self::Advanced,
            "theme" => Self::Theme,
            "details_motion" => Self::DetailsMotion,
            _ => panic!("unknown scenario {value:?}"),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Route => "route",
            Self::Reverse => "reverse",
            Self::Notifications => "notifications",
            Self::Typing => "typing",
            Self::Save => "save",
            Self::Advanced => "advanced",
            Self::Theme => "theme",
            Self::DetailsMotion => "details_motion",
        }
    }
    fn page(self) -> Option<&'static str> {
        match self {
            Self::Reverse => Some("Activity"),
            Self::Notifications | Self::Typing | Self::Save | Self::Advanced => Some("Preferences"),
            _ => None,
        }
    }
    fn control(self) -> Option<(SemanticRole, &'static str)> {
        match self {
            Self::Reverse => Some((SemanticRole::Button, "Reverse order")),
            Self::Notifications => Some((SemanticRole::CheckBox, "Enable notifications")),
            Self::Typing => Some((SemanticRole::TextInput, "Workspace name")),
            Self::Save => Some((SemanticRole::Button, "Save changes")),
            Self::Advanced => Some((SemanticRole::CheckBox, "Show advanced options")),
            Self::Theme => Some((SemanticRole::CheckBox, "Dark theme")),
            _ => None,
        }
    }
}

fn prepare<V: View>(source: &mut Source<V>, text: &mut TextContext) {
    source.experiment_prepare_without_scene(text, 1100, 820);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn target<V: View>(source: &Source<V>, role: SemanticRole, label: &str) -> u64 {
    source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(role, label, None)
        .unwrap_or_else(|| panic!("missing {role:?} {label:?}"))
        .id
}
fn click<V: View>(source: &mut Source<V>, role: SemanticRole, label: &str) {
    let id = target(source, role, label);
    assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
}
fn fingerprint<V: View>(source: &Source<V>) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for node in source
        .adapter
        .semantics(&source.engine, 0)
        .exposed_nodes()
        .into_iter()
        .filter(|node| {
            matches!(
                node.role,
                SemanticRole::Label
                    | SemanticRole::Heading
                    | SemanticRole::Button
                    | SemanticRole::TextInput
                    | SemanticRole::CheckBox
            )
        })
    {
        format!(
            "{:?}",
            (
                node.role,
                &node.label,
                &node.value,
                node.disabled,
                node.states,
                node.bounds
            )
        )
        .hash(&mut hash);
    }
    hash.finish()
}

fn bench<V: BenchView>(mode: &str, scenario: Scenario, options: &Options) -> u64 {
    let clock = VirtualClock::default();
    let mut view = V::default();
    if scenario != Scenario::DetailsMotion {
        view.reduce_motion();
    }
    let mut source = Source::with_clock(view, clock.clone());
    source.set_reduced_motion(scenario != Scenario::DetailsMotion);
    let mut text = TextContext::new();
    prepare(&mut source, &mut text);
    if let Some(page) = scenario.page() {
        click(&mut source, SemanticRole::Button, page);
        prepare(&mut source, &mut text);
    }
    let fixed = scenario
        .control()
        .map(|(role, label)| target(&source, role, label));
    if scenario == Scenario::Typing {
        assert!(source.handle_semantic_action(SemanticAction::Focus {
            target: fixed.unwrap()
        }));
        prepare(&mut source, &mut text);
    }
    if scenario == Scenario::DetailsMotion {
        click(&mut source, SemanticRole::Button, "Details");
        prepare(&mut source, &mut text);
    }

    let iterations = if scenario == Scenario::DetailsMotion {
        options.iterations.min(30)
    } else {
        options.iterations
    };
    let warmup = if scenario == Scenario::DetailsMotion {
        0
    } else {
        options.warmup
    };
    #[cfg(feature = "profiling")]
    let mut phases = [0_u128; 8];
    let mut samples = Vec::with_capacity(iterations);
    let (mut event_ns, mut declare_us, mut sync_us, mut layout_us) =
        (0_u128, 0_u128, 0_u128, 0_u128);
    let (mut visited, mut flattened, mut layout_runs, mut sync_visits, mut passes) =
        (0_usize, 0_usize, 0_usize, 0_usize, 0_usize);
    let (mut batches, mut retained) = (0_usize, 0_usize);
    for index in 0..warmup + iterations {
        if scenario == Scenario::DetailsMotion {
            clock.advance(Duration::from_micros(16_667)).unwrap();
        }
        let before = source.batch_stats();
        #[cfg(feature = "profiling")]
        support::reset();
        let start = Instant::now();
        match scenario {
            Scenario::Idle | Scenario::DetailsMotion => {}
            Scenario::Route => {
                let label = match index % 3 {
                    0 => "Activity",
                    1 => "Preferences",
                    _ => "Overview",
                };
                click(&mut source, SemanticRole::Button, label);
            }
            Scenario::Typing => {
                let result = if index.is_multiple_of(2) {
                    source.handle_event(UiEvent::TextInput("x".into()))
                } else {
                    source.handle_event(UiEvent::Key(KeyEvent {
                        phase: KeyPhase::Down,
                        key: "Backspace".into(),
                        code: "Backspace".into(),
                        key_without_modifiers: "Backspace".into(),
                        text: None,
                        text_with_all_modifiers: None,
                        location: KeyLocation::Standard,
                        modifiers: Modifiers::empty(),
                        repeat: false,
                        synthetic: false,
                    }))
                };
                assert!(result.handled);
            }
            _ => {
                assert!(source.handle_semantic_action(SemanticAction::Click {
                    target: fixed.unwrap()
                }));
            }
        }
        let event = start.elapsed().as_nanos();
        prepare(&mut source, &mut text);
        let elapsed = start.elapsed().as_nanos();
        black_box(&source.adapter.placed);
        let after = source.batch_stats();
        if index >= warmup {
            #[cfg(feature = "profiling")]
            support::accumulate(&mut phases);
            samples.push(elapsed);
            event_ns += event;
            declare_us += source.declaration_micros;
            sync_us += source.adapter.stats.sync_micros;
            layout_us += source.adapter.stats.layout_micros;
            visited += source.declaration_stats.visited;
            flattened += source.adapter.stats.flatten_visits;
            layout_runs += source.adapter.stats.layout_runs;
            sync_visits += source.adapter.stats.sync_visits;
            passes += after.declaration_passes - before.declaration_passes;
            batches += after.declaration_batches - before.declaration_batches;
            retained += after.retained_samples - before.retained_samples;
        }
    }
    samples.sort_unstable();
    let n = iterations as f64;
    let final_fingerprint = fingerprint(&source);
    println!(
        "{mode},{},{},{},{:.0},{:.2},{:.2},{:.2},{:.1},{:.1},{:.2},{:.1},{:.2},{:.2},{:.2},{},{:016x}",
        scenario.name(),
        samples[iterations / 2],
        samples[(iterations * 95 / 100).min(iterations - 1)],
        event_ns as f64 / n,
        declare_us as f64 / n,
        sync_us as f64 / n,
        layout_us as f64 / n,
        visited as f64 / n,
        flattened as f64 / n,
        layout_runs as f64 / n,
        sync_visits as f64 / n,
        passes as f64 / n,
        batches as f64 / n,
        retained as f64 / n,
        source.experiment_live_counts().0,
        final_fingerprint,
    );
    #[cfg(feature = "profiling")]
    support::report(
        &format!("mode={mode} scenario={}", scenario.name()),
        phases,
        iterations,
    );
    final_fingerprint
}

fn main() {
    #[cfg(feature = "profiling")]
    support::install();
    let options = Options::parse();
    assert!(options.iterations > 0, "iterations must be positive");
    println!(
        "mode,scenario,p50_total_ns,p95_total_ns,mean_event_ns,mean_declare_us,mean_sync_us,mean_layout_us,mean_visited,mean_flatten_visits,mean_layout_runs,mean_sync_visits,mean_declaration_passes,mean_declaration_batches,mean_retained_samples,nodes,final_fingerprint"
    );
    for name in options.scenarios.split(',') {
        let scenario = Scenario::parse(name);
        let immediate = bench::<ImmediateSettings>("immediate", scenario, &options);
        let reactive = bench::<Settings>("reactive", scenario, &options);
        if scenario != Scenario::DetailsMotion {
            assert_eq!(
                immediate,
                reactive,
                "semantic mismatch in {}",
                scenario.name()
            );
        }
    }
}
