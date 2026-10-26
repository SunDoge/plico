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
        self.refresh_snapshot();
        let handled = self.app.on_key_event(key, &mut self.context);
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
        let sent = !commands.is_empty();
        for command in commands {
            self.services.send(command);
        }
        // Enqueuing asynchronous work does not change the current snapshot.
        // A fresh projection will wake us through poll_reload/revision. Keep
        // eager settling for legacy unversioned or synchronously updated hosts.
        if sent
            && self
                .services
                .snapshot_revision()
                .is_none_or(|revision| Some(revision) != self.snapshot_revision)
        {
            ui.request_rebuild();
        }
    }
}
