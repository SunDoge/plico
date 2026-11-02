use plico::{UiContext, prelude::*};
use plico_desktop::{HostView, ServiceView, Source};
use plico_dev::services::{HostService, ServiceApp, Services, Wake};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Default)]
struct Actor {
    pending: Vec<u64>,
    operations: Vec<u64>,
    snapshot: u64,
    revision: u64,
    reads: Cell<usize>,
    wake: Option<Wake>,
    race: bool,
    commands: Vec<u64>,
}
impl HostService for Actor {
    type Snapshot = u64;
    type Command = u64;
    fn snapshot(&self) -> u64 {
        self.reads.set(self.reads.get() + 1);
        self.snapshot
    }
    fn snapshot_revision(&self) -> Option<u64> {
        Some(self.revision)
    }
    fn send(&mut self, command: u64) {
        self.commands.push(command);
    }
    fn set_wake_callback(&mut self, wake: Wake) {
        self.wake = Some(wake);
    }
    fn poll(&mut self) -> bool {
        let changed = !self.pending.is_empty();
        for value in self.pending.drain(..) {
            self.operations.push(value);
            self.snapshot = value;
            self.revision += 1;
        }
        // Simulate a completion arriving after the last read in this drain.
        if std::mem::take(&mut self.race) {
            self.pending.push(101);
            self.wake.as_ref().unwrap()();
        }
        changed
    }
}
#[derive(Default, Serialize, Deserialize)]
struct App {
    calls: usize,
    observed: u64,
    send: bool,
}
impl ServiceApp for App {
    type Snapshot = u64;
    type Command = u64;
    fn show(&mut self, cx: &mut UiContext, services: &mut Services<u64, u64>) {
        self.calls += 1;
        self.observed = *services.snapshot();
        text!(cx, "Progress {}", self.observed);
        if std::mem::take(&mut self.send) {
            for command in [7, 8, 9] {
                services.send(command);
            }
        }
    }
}
#[test]
fn burst_drains_operations_but_projects_only_final_state() {
    let mut source = Source::new(ServiceView::new(App::default(), Actor::default()));
    let scheduled = Arc::new(AtomicUsize::new(0));
    let count = scheduled.clone();
    source.set_wake_callback(Arc::new(move || {
        count.fetch_add(1, Ordering::Relaxed);
    }));
    let mut tcx = TextContext::new();
    source.poll_async(); // Drain startup notification.
    source.build_frame(&mut tcx, 400, 300);
    let before = source.batch_stats();
    for value in 1..=100 {
        source.view.services.pending.push(value);
        source.view.services.wake.as_ref().unwrap()();
        source.request_rebuild();
    }
    assert_eq!(
        source.batch_stats().scheduled_wakes - before.scheduled_wakes,
        1
    );
    assert_eq!(
        source.batch_stats().wake_notifications - before.wake_notifications,
        100
    );
    assert!(source.poll_async());
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.app.observed, 100);
    assert_eq!(
        source.view.services.operations,
        (1..=100).collect::<Vec<_>>()
    );
    assert_eq!(source.view.services.reads.get(), 2);
    let after = source.batch_stats();
    assert_eq!(after.rebuild_requests - before.rebuild_requests, 100);
    assert_eq!(after.declaration_batches - before.declaration_batches, 1);
    assert_eq!(after.declaration_passes - before.declaration_passes, 1);
    assert_eq!(after.backend_commits - before.backend_commits, 1);
    assert_eq!(after.layout_runs - before.layout_runs, 1);
    assert_eq!(scheduled.load(Ordering::Relaxed), 2);

    // Re-arming before poll avoids losing this notification.
    source.view.services.race = true;
    let before = source.batch_stats();
    assert!(!source.poll_async());
    assert_eq!(
        source.batch_stats().scheduled_wakes - before.scheduled_wakes,
        1
    );
    assert!(source.poll_async());
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.app.observed, 101);
}
#[test]
fn async_commands_preserve_order_without_resettling_unchanged_snapshot() {
    let mut source = Source::new(ServiceView::new(App::default(), Actor::default()));
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 400, 300);
    let before = source.batch_stats();
    source.view.app.send = true;
    source.request_rebuild();
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.services.commands, [7, 8, 9]);
    let after = source.batch_stats();
    assert_eq!(after.declaration_batches - before.declaration_batches, 1);
    assert_eq!(after.declaration_passes - before.declaration_passes, 1);
    assert_eq!(after.backend_commits - before.backend_commits, 1);
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.services.commands, [7, 8, 9]);
}
#[derive(Default)]
struct Counter(usize);
impl HostView for Counter {
    fn show(&mut self, cx: &mut UiContext) {
        if button(cx).text("Increment").clicked() {
            self.0 += 1;
        }
        text!(cx, "{}", self.0);
    }
}
#[test]
fn input_boundaries_preserve_repeated_clicks() {
    let mut source = Source::new(Counter::default());
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 400, 300);
    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Increment", None)
        .unwrap()
        .id;
    let before = source.batch_stats();
    for _ in 0..3 {
        source.prepare_for_event(&mut tcx);
        assert!(source.handle_semantic_action(SemanticAction::Click { target }));
    }
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.0, 3);
    assert_eq!(source.batch_stats().event_flushes - before.event_flushes, 2);
}

#[test]
fn concurrent_notifications_schedule_one_wake_until_poll() {
    let mut source = Source::new(ServiceView::new(App::default(), Actor::default()));
    let scheduled = Arc::new(AtomicUsize::new(0));
    let count = scheduled.clone();
    source.set_wake_callback(Arc::new(move || {
        count.fetch_add(1, Ordering::Relaxed);
    }));
    source.poll_async();
    let before = source.batch_stats();
    let wake = source.view.services.wake.clone().unwrap();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let wake = wake.clone();
            scope.spawn(move || {
                for _ in 0..100 {
                    wake();
                }
            });
        }
    });
    let after = source.batch_stats();
    assert_eq!(after.wake_notifications - before.wake_notifications, 800);
    assert_eq!(after.scheduled_wakes - before.scheduled_wakes, 1);
    assert_eq!(scheduled.load(Ordering::Relaxed), 2);
    source.poll_async();
    wake();
    assert_eq!(scheduled.load(Ordering::Relaxed), 3);
}

#[test]
fn conditional_async_load_settles_and_native_frame_observes_completion() {
    #[derive(Default, Serialize, Deserialize)]
    struct Loader;
    impl ServiceApp for Loader {
        type Snapshot = u64;
        type Command = u64;
        fn show(&mut self, cx: &mut UiContext, services: &mut Services<u64, u64>) {
            if *services.snapshot() == 0 {
                services.send(42);
            }
            text!(cx, "Loaded {}", services.snapshot());
        }
    }
    let mut source = Source::new(ServiceView::new(Loader, Actor::default()));
    let mut text = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (400, 300), 1.)
        .unwrap();
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    assert_eq!(source.view.services.commands, [42]);
    assert_eq!(source.batch_stats().declaration_passes, 1);
    source.view.services.pending.push(42);
    assert!(source.poll_async());
    source
        .render_retained_frame(&mut scene, &mut text, (400, 300), 1.)
        .unwrap();
    assert!(source.last_error.is_none());
    assert_eq!(source.view.services.commands, [42]);
    assert!(
        source
            .adapter
            .placed
            .iter()
            .any(|n| n.paint.text.as_deref() == Some("Loaded 42"))
    );
}
