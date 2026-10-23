#![cfg(feature = "desktop")]
use plico_dev::services::HostService;
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_services_demo::{Application, TaskCommand, TaskSnapshot, TaskStatus};
use plico_vello::{ServiceView, Source};
#[derive(Default)]
struct Resident {
    snapshot: TaskSnapshot,
    sends: usize,
    pending_completion: bool,
}
impl HostService for Resident {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;
    fn snapshot(&self) -> TaskSnapshot {
        self.snapshot.clone()
    }
    fn poll(&mut self) -> bool {
        if !std::mem::take(&mut self.pending_completion) {
            return false;
        }
        self.snapshot.status = TaskStatus::Completed;
        self.snapshot.completed += 1;
        true
    }
    fn send(&mut self, command: TaskCommand) {
        self.sends += 1;
        match command {
            TaskCommand::Start => {
                self.snapshot.started += 1;
                self.snapshot.status = TaskStatus::Running;
            }
            TaskCommand::Cancel => self.snapshot.status = TaskStatus::Cancelled,
        }
    }
}
#[test]
fn static_host_uses_same_typed_ui_without_serializing_or_repeating_service_actions() {
    let mut source = Source::new(ServiceView::new(
        Application::default(),
        Resident::default(),
    ));
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 620, 520);
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Start task", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Click { target: id });
    for _ in 0..4 {
        source.build_frame(&mut tcx, 620, 520);
        assert!(source.last_error.is_none());
    }
    assert_eq!(source.view.services.sends, 1);
    assert_eq!(source.view.services.snapshot().started, 1);
    let node = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Start task", None)
        .unwrap()
        .clone();
    assert!(node.disabled);
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Cancel task", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Click { target: id });
    source.build_frame(&mut tcx, 620, 520);
    assert_eq!(source.view.services.sends, 2);
    assert_eq!(
        source.view.services.snapshot().status,
        TaskStatus::Cancelled
    );
}

#[test]
fn queued_startup_completion_is_projected_before_the_first_frame() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let mut source = Source::new(ServiceView::new(
        Application::default(),
        Resident {
            pending_completion: true,
            ..Default::default()
        },
    ));
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    source.set_wake_callback(Arc::new(move || {
        count.fetch_add(1, Ordering::Relaxed);
    }));
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 620, 520);
    assert_eq!(source.view.services.snapshot().completed, 1);
    assert_eq!(
        source.view.services.snapshot().status,
        TaskStatus::Completed
    );
    assert!(!source.poll_async());
    assert_eq!(source.view.services.snapshot().completed, 1);
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct ProjectionApp {
    observed: u64,
}
impl plico_dev::services::ServiceApp for ProjectionApp {
    type Snapshot = u64;
    type Command = ();
    fn show(
        &mut self,
        cx: &mut plico_core::Ui<'_>,
        services: &mut plico_dev::services::Services<u64, ()>,
    ) {
        self.observed = *services.snapshot();
        cx.text("Projection");
        if cx.button("Advance").clicked() {
            services.send(());
        }
    }
}
#[derive(Default)]
struct VersionedService {
    value: u64,
    reads: std::cell::Cell<usize>,
    versioned: bool,
}
impl HostService for VersionedService {
    type Snapshot = u64;
    type Command = ();
    fn snapshot(&self) -> u64 {
        self.reads.set(self.reads.get() + 1);
        self.value
    }
    fn snapshot_revision(&self) -> Option<u64> {
        self.versioned.then_some(self.value)
    }
    fn send(&mut self, _: ()) {
        self.value += 1;
    }
}
#[test]
fn versioned_snapshots_are_reused_until_revision_changes() {
    let mut source = Source::new(ServiceView::new(
        ProjectionApp::default(),
        VersionedService {
            versioned: true,
            ..Default::default()
        },
    ));
    let mut tcx = TextContext::new();
    for _ in 0..10 {
        source.request_rebuild();
        source.build_frame(&mut tcx, 400, 300);
    }
    assert_eq!(source.view.services.reads.get(), 1);
    source.view.services.send(());
    source.request_rebuild();
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.app.observed, 1);
    assert_eq!(source.view.services.reads.get(), 2);
    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Advance", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Click { target });
    source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.app.observed, 2);
    assert_eq!(source.view.services.reads.get(), 3);
    // Returning None opts back into eager generation without changing the value.
    source.view.services.versioned = false;
    for _ in 0..2 {
        source.request_rebuild();
        source.build_frame(&mut tcx, 400, 300);
    }
    assert_eq!(source.view.services.reads.get(), 5);
}
