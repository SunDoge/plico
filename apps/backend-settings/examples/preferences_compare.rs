//! Native CPU pipeline, no window, Scene encoding or GPU. Run in release mode.
use backend_settings::preferences_regions::{ImmediatePreferences, RegionPreferences};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext, UiEvent};
use plico_runtime::{HostView, Source};
use std::{hint::black_box, time::Instant};
#[derive(usage_rs::Cli)]
#[usage(about = "Compare the same settings interactions through both APIs (no GPU)")]
struct Options {
    #[usage(long, default = "2000")]
    iterations: usize,
}
fn prepare<V: HostView>(source: &mut Source<V>, text: &mut TextContext) {
    source.experiment_prepare_without_scene(text, 900, 780);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn target<V: HostView>(source: &Source<V>, role: SemanticRole, label: &str) -> u64 {
    source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(role, label, None)
        .unwrap()
        .id
}
fn bench<V: HostView + Default>(mode: &str, scenario: &str, iterations: usize) {
    let mut source = Source::new(V::default());
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    prepare(&mut source, &mut text);
    let role = if matches!(scenario, "save" | "reset") {
        SemanticRole::Button
    } else if scenario == "typing" {
        SemanticRole::TextInput
    } else {
        SemanticRole::CheckBox
    };
    let label = match scenario {
        "typing" => "Workspace name",
        "notifications" => "Enable notifications",
        "advanced" => "Show advanced options",
        "theme" => "Dark theme",
        "save" => "Save changes",
        "reset" => "Reset name",
        _ => unreachable!(),
    };
    let id = target(&source, role, label);
    if scenario == "typing" {
        assert!(source.handle_semantic_action(SemanticAction::Focus { target: id }));
        prepare(&mut source, &mut text);
    }
    let mut samples = Vec::with_capacity(iterations);
    let mut events = 0_u128;
    let mut declaration = 0_u128;
    let mut sync = 0_u128;
    let mut layout = 0_u128;
    let mut visited = 0_usize;
    let mut flattened = 0_usize;
    let mut layout_runs = 0_usize;
    let mut sync_visits = 0_usize;
    let mut passes = 0_usize;
    // Warm fonts, shaping, both branch states and allocator capacity equally.
    for index in 0..iterations + 200 {
        // A reset is measured after a real edit, not as a repeated no-op.
        if scenario == "reset" {
            let input = target(&source, SemanticRole::TextInput, "Workspace name");
            assert!(source.handle_semantic_action(SemanticAction::Focus { target: input }));
            prepare(&mut source, &mut text);
            assert!(source.handle_event(UiEvent::TextInput("x".into())).handled);
            prepare(&mut source, &mut text);
        }
        let before = source.batch_stats().declaration_passes;
        let start = Instant::now();
        if scenario == "typing" {
            if index % 2 == 0 {
                source.handle_event(UiEvent::TextInput("x".into()));
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
                }));
            }
        } else {
            assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
        }
        let event = start.elapsed().as_nanos();
        prepare(&mut source, &mut text);
        let elapsed = start.elapsed().as_nanos();
        black_box(&source.adapter.placed);
        if index >= 200 {
            samples.push(elapsed);
            events += event;
            declaration += source.declaration_micros;
            sync += source.adapter.stats.sync_micros;
            layout += source.adapter.stats.layout_micros;
            visited += source.declaration_stats.visited;
            flattened += source.adapter.stats.flatten_visits;
            layout_runs += source.adapter.stats.layout_runs;
            sync_visits += source.adapter.stats.sync_visits;
            passes += source.batch_stats().declaration_passes - before;
        }
    }
    samples.sort_unstable();
    let n = iterations as f64;
    println!(
        "{mode},{scenario},{},{},{:.0},{:.2},{:.2},{:.2},{:.1},{:.1},{:.1},{:.1},{:.1},{}",
        samples[iterations / 2],
        samples[(iterations * 95 / 100).min(iterations - 1)],
        events as f64 / n,
        declaration as f64 / n,
        sync as f64 / n,
        layout as f64 / n,
        visited as f64 / n,
        flattened as f64 / n,
        layout_runs as f64 / n,
        sync_visits as f64 / n,
        passes as f64 / n,
        source.experiment_live_counts().0
    );
}
fn main() {
    let options = Options::parse();
    assert!(options.iterations > 0, "iterations must be positive");
    println!(
        "mode,scenario,p50_total_ns,p95_total_ns,mean_event_ns,mean_declare_us,mean_sync_us,mean_layout_us,mean_visited_last_pass_or_region_batch,mean_flatten_visits,mean_layout_runs,mean_sync_visits,mean_declaration_passes,nodes"
    );
    for scenario in [
        "notifications",
        "typing",
        "save",
        "reset",
        "advanced",
        "theme",
    ] {
        bench::<ImmediatePreferences>("immediate", scenario, options.iterations);
        bench::<RegionPreferences>("regions", scenario, options.iterations);
    }
}
