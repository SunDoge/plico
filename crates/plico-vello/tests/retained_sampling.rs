use plico_core::{Theme, Ui, VirtualClock};
use plico_renderer::{FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
use plico_vello::{Source, View};
use std::time::Duration;

#[derive(Default)]
struct Demo {
    calls: usize,
    count: usize,
    incoming: Option<usize>,
    dark: bool,
    invalid: bool,
    logical_animation: bool,
    fade: bool,
}
impl View for Demo {
    fn show(&mut self, ui: &mut Ui<'_>) {
        self.calls += 1;
        plico_core::text!(ui, "Count {}", self.count)
            .key("label")
            .opacity(if self.invalid {
                -1.
            } else if self.fade {
                0.2
            } else {
                1.
            })
            .transition(200);
        if ui.button("Increment").key("increment").clicked() {
            self.count += 1;
        }
        if self.logical_animation && ui.now() < Duration::from_millis(100) {
            ui.request_animation_frame();
        }
    }
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::default()
        }
    }
    fn poll_reload(&mut self) -> bool {
        if let Some(value) = self.incoming.take() {
            self.count = value;
            true
        } else {
            false
        }
    }
}
fn sample(source: &mut Source<Demo>, text: &mut TextContext, size: (u32, u32), scale: f64) {
    source
        .render_retained_frame(&mut Scene::new(), text, size, scale)
        .unwrap();
}
#[test]
fn native_idle_samples_reuse_declarations_while_snapshot_api_still_evaluates() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    sample(&mut source, &mut text, (400, 300), 1.);
    let before = source.batch_stats();
    let calls = source.view.calls;
    for _ in 0..120 {
        clock.advance(Duration::from_millis(16)).unwrap();
        sample(&mut source, &mut text, (400, 300), 1.);
        assert_eq!(source.declaration_stats.visited, 0);
        assert_eq!(source.declaration_micros, 0);
        assert_eq!(source.adapter.stats.layout_runs, 0);
        assert_eq!(source.adapter.stats.scene_reuses, 1);
    }
    let after = source.batch_stats();
    assert_eq!(source.view.calls, calls);
    assert_eq!(after.retained_samples - before.retained_samples, 120);
    assert_eq!(after.backend_commits, before.backend_commits);
    assert_eq!(after.declaration_batches, before.declaration_batches);
    assert!(!source.has_anim());
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.calls, calls + 1);
}
#[test]
fn inputs_services_resize_theme_and_font_changes_invalidate_native_samples() {
    let mut source = Source::with_clock(Demo::default(), VirtualClock::default());
    let mut text = TextContext::new();
    sample(&mut source, &mut text, (400, 300), 1.);
    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Increment", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target }));
    sample(&mut source, &mut text, (400, 300), 1.);
    assert_eq!(source.view.count, 1);
    assert!(
        source
            .adapter
            .placed
            .iter()
            .any(|n| n.paint.text.as_deref() == Some("Count 1"))
    );
    let mut calls = source.view.calls;
    source.view.incoming = Some(42);
    assert!(source.poll_async());
    sample(&mut source, &mut text, (400, 300), 1.);
    assert_eq!(source.view.calls, calls + 1);
    calls += 1;
    sample(&mut source, &mut text, (500, 300), 1.);
    assert_eq!(source.view.calls, calls + 1);
    calls += 1;
    sample(&mut source, &mut text, (500, 300), 2.);
    assert_eq!(source.view.calls, calls); // DPI-only changes still re-encode the scene.
    assert_eq!(source.adapter.stats.scene_builds, 1);
    source.view.dark = true;
    sample(&mut source, &mut text, (500, 300), 2.);
    assert_eq!(source.view.calls, calls + 1);
    calls += 1;
    source.set_font_family("monospace");
    sample(&mut source, &mut text, (500, 300), 2.);
    assert_eq!(source.view.calls, calls + 1);
    calls += 1;
    for _ in 0..100 {
        source.request_rebuild();
    }
    sample(&mut source, &mut text, (500, 300), 2.);
    assert_eq!(source.view.calls, calls + 1);
}
#[test]
fn logical_animation_rebuilds_until_finished_and_host_animation_does_not() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            logical_animation: true,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    sample(&mut source, &mut text, (400, 300), 1.);
    let calls = source.view.calls;
    for _ in 0..4 {
        clock.advance(Duration::from_millis(25)).unwrap();
        sample(&mut source, &mut text, (400, 300), 1.);
    }
    assert_eq!(source.view.calls, calls + 4);
    assert!(!source.has_anim());
    sample(&mut source, &mut text, (400, 300), 1.);
    assert_eq!(source.view.calls, calls + 4);
    source.view.fade = true;
    source.request_rebuild();
    sample(&mut source, &mut text, (400, 300), 1.);
    let calls = source.view.calls;
    assert!(source.has_anim());
    for _ in 0..3 {
        clock.advance(Duration::from_millis(100)).unwrap();
        sample(&mut source, &mut text, (400, 300), 1.);
        assert_eq!(source.view.calls, calls);
    }
    assert!(!source.has_anim());
    assert_eq!(source.adapter.stats.scene_reuses, 1);
}
#[test]
fn failed_declarations_retry_and_can_recover_without_reusing_a_partial_frame() {
    let mut source = Source::with_clock(
        Demo {
            invalid: true,
            ..Default::default()
        },
        VirtualClock::default(),
    );
    let mut text = TextContext::new();
    for _ in 0..2 {
        sample(&mut source, &mut text, (400, 300), 1.);
        assert!(source.last_error.is_some());
    }
    assert_eq!(source.view.calls, 2);
    source.view.invalid = false;
    sample(&mut source, &mut text, (400, 300), 1.);
    assert!(source.last_error.is_none());
    assert_eq!(source.view.calls, 3);
}

#[test]
fn manually_committed_declarations_are_synced_without_being_executed_again() {
    let mut source = Source::with_clock(Demo::default(), VirtualClock::default());
    let mut text = TextContext::new();
    sample(&mut source, &mut text, (400, 300), 1.);
    source.view.count = 123;
    source.try_declare().unwrap();
    let calls = source.view.calls;
    sample(&mut source, &mut text, (400, 300), 1.);
    assert_eq!(source.view.calls, calls);
    assert!(
        source
            .adapter
            .placed
            .iter()
            .any(|n| n.paint.text.as_deref() == Some("Count 123"))
    );
    assert_eq!(source.adapter.stats.sync_visits, 1);
}

#[test]
fn external_updates_run_every_operation_but_coalesce_into_one_declaration() {
    let mut source = Source::with_clock(Demo::default(), VirtualClock::default());
    let mut text = TextContext::new();
    sample(&mut source, &mut text, (400, 300), 1.);
    let calls = source.view.calls;
    for _ in 0..100 {
        source.update(|view| view.count += 1);
    }
    assert_eq!(source.view.count, 100);
    sample(&mut source, &mut text, (400, 300), 1.);
    assert_eq!(source.view.calls, calls + 1);
    assert!(
        source
            .adapter
            .placed
            .iter()
            .any(|n| n.paint.text.as_deref() == Some("Count 100"))
    );
}
