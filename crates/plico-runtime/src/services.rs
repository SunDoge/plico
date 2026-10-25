//! Resident service ownership for statically linked and replaceable application views.
use crate::View;
use plico_dev::services::{HostService, ServiceApp, Services};

pub struct ServiceView<A: ServiceApp, S: HostService<Snapshot = A::Snapshot, Command = A::Command>>
{
    pub app: A,
    pub services: S,
    context: Services<A::Snapshot, A::Command>,
    snapshot_revision: Option<u64>,
}
impl<A: ServiceApp, S: HostService<Snapshot = A::Snapshot, Command = A::Command>>
    ServiceView<A, S>
{
    pub fn new(app: A, mut services: S) -> Self {
        services.poll();
        let context = Services::new(services.snapshot());
        let snapshot_revision = services.snapshot_revision();
        Self {
            app,
            services,
            context,
            snapshot_revision,
        }
    }
    fn refresh_snapshot(&mut self) {
        let revision = self.services.snapshot_revision();
        if revision.is_none() || revision != self.snapshot_revision {
            self.context.replace_snapshot(self.services.snapshot());
            self.snapshot_revision = revision;
        }
    }
}
impl<A: ServiceApp, S: HostService<Snapshot = A::Snapshot, Command = A::Command>> View
    for ServiceView<A, S>
{
    fn snapshot_changed(&self) -> bool {
        self.services
            .snapshot_revision()
            .is_some_and(|revision| Some(revision) != self.snapshot_revision)
    }
    fn set_frame_time(&mut self, time: plico_core::FrameTime) {
        self.app.set_frame_time(time);
    }
    fn set_wake_callback(&mut self, wake: crate::WakeCallback) {
        self.services.set_wake_callback(wake.clone());
        wake();
    }
    fn poll_reload(&mut self) -> bool {
        self.services.poll()
    }
    fn theme(&self) -> plico_core::Theme {
        self.app.theme()
    }
    fn on_unhandled_key(&mut self, key: &crate::KeyEvent) -> bool {
        if !key.modifiers.is_empty() {
            return false;
        }
        self.refresh_snapshot();
        let handled = self.app.on_key(&key.key, &mut self.context);
        let commands = self.context.take_commands();
        if handled {
            for command in commands {
                self.services.send(command);
            }
        }
        handled
    }
    fn show(&mut self, ui: &mut plico_core::UiContext<'_>) {
        self.refresh_snapshot();
        self.app.show(ui, &mut self.context);
        let commands = self.context.take_commands();
        if !commands.is_empty() {
            ui.request_rebuild();
        }
        for command in commands {
            self.services.send(command);
        }
    }
}
