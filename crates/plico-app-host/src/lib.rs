//! Thin bridge from [`plico_dev`] application/service protocols to
//! [`plico_runtime::HostView`]. The runtime itself does not depend on the
//! development and value-protocol layer.

use plico_dev::services::{HostService, ServiceApp, Services};
use plico_runtime::{HostView, KeyEvent, WakeCallback};

/// A statically linked [`plico_dev::App`] adapted to the native host contract.
pub struct NativeApp<A>(pub A);

/// Shared forwarding for both [`NativeApp`] and application-owned wrappers.
#[doc(hidden)]
#[macro_export]
macro_rules! __plico_app_host_view_methods {
    ($this:ident, $app:expr, $shared:expr) => {
        fn set_frame_time(&mut $this, time: $crate::__ui_core::FrameTime) {
            $crate::__app_protocol::App::set_frame_time($app, time);
        }
        fn on_unhandled_key(&mut $this, key: &$crate::__runtime::KeyEvent) -> bool {
            $crate::__app_protocol::App::on_key_event($app, key)
        }
        fn show(&mut $this, ui: &mut $crate::__ui_core::UiContext<'_>) {
            $crate::__app_protocol::App::show($app, ui);
        }
        fn theme(&$this) -> $crate::__ui_core::Theme {
            $crate::__app_protocol::App::theme($shared)
        }
    };
}

/// Implement [`plico_runtime::HostView`] for an application using the same
/// forwarding as [`NativeApp`].
#[macro_export]
macro_rules! impl_host_view_for_app {
    ($app:ty) => {
        impl $crate::__runtime::HostView for $app {
            $crate::__plico_app_host_view_methods!(self, self, self);
        }
    };
}

impl<A: plico_dev::App> HostView for NativeApp<A> {
    crate::__plico_app_host_view_methods!(self, &mut self.0, &self.0);
}

/// Resident service ownership for a typed [`ServiceApp`].
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

impl<A: ServiceApp, S: HostService<Snapshot = A::Snapshot, Command = A::Command>> HostView
    for ServiceView<A, S>
{
    fn snapshot_changed(&self) -> bool {
        self.services
            .snapshot_revision()
            .is_some_and(|revision| Some(revision) != self.snapshot_revision)
    }

    fn set_frame_time(&mut self, time: plico::FrameTime) {
        self.app.set_frame_time(time);
    }

    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.services.set_wake_callback(wake.clone());
        wake();
    }

    fn poll_updates(&mut self) -> bool {
        self.services.poll()
    }

    fn theme(&self) -> plico::Theme {
        self.app.theme()
    }

    fn on_unhandled_key(&mut self, key: &KeyEvent) -> bool {
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

    fn show(&mut self, ui: &mut plico::UiContext<'_>) {
        self.refresh_snapshot();
        self.app.show(ui, &mut self.context);
        let commands = self.context.take_commands();
        let sent = !commands.is_empty();
        for command in commands {
            self.services.send(command);
        }
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

/// Task protocol types remain owned by `plico-dev`; hosts may import them
/// through this bridge when they already use [`ServiceView`].
pub mod tasks {
    pub use plico_dev::tasks::*;
}

#[doc(hidden)]
pub use plico as __ui_core;
#[doc(hidden)]
pub use plico_dev as __app_protocol;
#[doc(hidden)]
pub use plico_runtime as __runtime;
