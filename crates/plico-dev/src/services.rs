//! Typed, value-only bridge. Host resources and callbacks never cross the module ABI.
use crate::{App, Frame, Request, Result};
use plico_core::{FrameTime, Theme, UiContext};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use std::sync::Arc;
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Implemented in the executable. `send` should enqueue work, never block for IO.
pub trait HostService {
    type Snapshot: Default + Serialize + DeserializeOwned;
    type Command: Serialize + DeserializeOwned;
    /// An explicit presentation projection, never an SDK session or native handle.
    fn snapshot(&self) -> Self::Snapshot;
    /// Optional projection version. Change it whenever snapshot data changes.
    /// None preserves eager snapshot generation.
    fn snapshot_revision(&self) -> Option<u64> {
        None
    }
    fn send(&mut self, command: Self::Command);
    /// Drain the available completion batch after a wake; return whether the
    /// projection changed. Apply every operation, even if only the final
    /// presentation state is exposed. A wake is a hint, not one queued message.
    fn poll(&mut self) -> bool {
        false
    }
    fn set_wake_callback(&mut self, _wake: Wake) {}
}
#[derive(Default)]
pub struct NoServices;
#[derive(Serialize, Deserialize)]
pub enum NoCommand {}
impl HostService for NoServices {
    type Snapshot = ();
    type Command = NoCommand;
    fn snapshot(&self) {}
    fn send(&mut self, command: NoCommand) {
        match command {}
    }
}

/// Borrow for one declaration. The snapshot is read-only; commands are an outbox.
/// Neither snapshot nor outbox is part of the application's serialized UI state.
pub struct Services<S, C> {
    snapshot: Arc<S>,
    commands: Vec<C>,
}
impl<S: Default, C> Default for Services<S, C> {
    fn default() -> Self {
        Self {
            snapshot: Arc::new(S::default()),
            commands: Vec::new(),
        }
    }
}
impl<S, C> Services<S, C> {
    pub fn new(snapshot: S) -> Self {
        Self {
            snapshot: Arc::new(snapshot),
            commands: Vec::new(),
        }
    }
    pub fn snapshot(&self) -> &S {
        &self.snapshot
    }
    /// Retain a read-only snapshot while emitting commands, without deep-cloning it.
    /// The Arc remains local to this Rust context; only values cross the module ABI.
    pub fn snapshot_shared(&self) -> Arc<S> {
        Arc::clone(&self.snapshot)
    }
    pub fn send(&mut self, command: C) {
        self.commands.push(command);
    }
    #[doc(hidden)]
    pub fn replace_snapshot(&mut self, snapshot: S) {
        self.snapshot = Arc::new(snapshot);
    }
    #[doc(hidden)]
    pub fn take_commands(&mut self) -> Vec<C> {
        std::mem::take(&mut self.commands)
    }
}

/// Reloadable UI with a typed host projection/message protocol. Only `Self` is restored.
pub trait ServiceApp: Default + Serialize + DeserializeOwned {
    type Snapshot: Default + Serialize + DeserializeOwned;
    type Command: Serialize + DeserializeOwned;
    fn show(
        &mut self,
        ui: &mut UiContext<'_>,
        services: &mut Services<Self::Snapshot, Self::Command>,
    );
    fn theme(&self) -> Theme {
        Theme::default()
    }
    fn set_frame_time(&mut self, _time: FrameTime) {}
    fn on_key(
        &mut self,
        _key: &str,
        _services: &mut Services<Self::Snapshot, Self::Command>,
    ) -> bool {
        false
    }
}
struct ServiceModule<A: ServiceApp> {
    app: A,
    services: Services<A::Snapshot, A::Command>,
}
impl<A: ServiceApp> Default for ServiceModule<A> {
    fn default() -> Self {
        Self {
            app: A::default(),
            services: Services::default(),
        }
    }
}
impl<A: ServiceApp> Serialize for ServiceModule<A> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.app.serialize(serializer)
    }
}
impl<'de, A: ServiceApp> Deserialize<'de> for ServiceModule<A> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Ok(Self {
            app: A::deserialize(deserializer)?,
            services: Services::default(),
        })
    }
}
impl<A: ServiceApp> App for ServiceModule<A> {
    fn set_services(&mut self, snapshot: serde_json::Value) -> Result<()> {
        let snapshot = if snapshot.is_null() {
            A::Snapshot::default()
        } else {
            serde_json::from_value(snapshot).map_err(|source| crate::Error::Json { source })?
        };
        self.services.replace_snapshot(snapshot);
        Ok(())
    }
    fn take_service_commands(&mut self) -> Result<Vec<serde_json::Value>> {
        self.services
            .take_commands()
            .into_iter()
            .map(|c| serde_json::to_value(c).map_err(|source| crate::Error::Json { source }))
            .collect()
    }
    fn show(&mut self, ui: &mut UiContext<'_>) {
        self.app.show(ui, &mut self.services);
    }
    fn theme(&self) -> Theme {
        self.app.theme()
    }
    fn set_frame_time(&mut self, time: FrameTime) {
        self.app.set_frame_time(time);
    }
    fn on_key(&mut self, key: &str) -> bool {
        self.app.on_key(key, &mut self.services)
    }
}
pub fn render<A: ServiceApp + 'static>(request: Request) -> Result<Frame> {
    crate::render::<ServiceModule<A>>(request)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn dispatch<A: ServiceApp + 'static>(input: crate::stabby::vec::Vec<u8>) -> crate::Reply {
    crate::dispatch_with(input, render::<A>)
}

/// Export one module entry point; all component internals remain ordinary Rust.
#[macro_export]
macro_rules! export_service_app {
    ($app:ty) => {
        #[$crate::stabby::export]
        pub extern "C" fn plico_app_render(input: $crate::stabby::vec::Vec<u8>) -> $crate::Reply {
            $crate::services::dispatch::<$app>(input)
        }
    };
}

#[cfg(test)]
mod shared_snapshot_tests {
    use super::*;
    #[test]
    fn commands_share_snapshot_and_replacement_keeps_previous_reads_valid() {
        struct Data(String); // Deliberately not Clone.
        let mut services = Services::<_, u8>::new(Data("previous".into()));
        let read = services.snapshot_shared();
        services.send(1);
        assert!(Arc::ptr_eq(&read, &services.snapshot_shared()));
        services.replace_snapshot(Data("next".into()));
        assert_eq!(read.0, "previous");
        assert_eq!(services.snapshot().0, "next");
        assert!(!Arc::ptr_eq(&read, &services.snapshot_shared()));
        assert_eq!(services.take_commands(), vec![1]);
    }
}
