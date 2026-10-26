//! Host-owned system-wide shortcuts. Native registrations live on the event-loop
//! thread; Wayland uses an asynchronous desktop portal and reports readiness.
use global_hotkey::hotkey::HotKey;
use snafu::Snafu;
use std::{
    collections::BTreeMap,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, mpsc},
};
mod native;
#[cfg(target_os = "linux")]
mod portal;
/// Callback which schedules the host event loop; it never runs UI declarations.
pub type Wake = Arc<dyn Fn() + Send + Sync>;
/// Registration or capability error. OS details are retained.
#[derive(Debug, Snafu)]
#[snafu(display("global shortcut: {message}"))]
pub struct Error {
    message: String,
}
impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
/// Platform implementation selected for this manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Windows RegisterHotKey, macOS native hotkeys, or X11 grabs.
    Native,
    /// Wayland XDG GlobalShortcuts portal; the desktop controls the actual keys.
    Portal,
}
/// The desktop may ask for approval before a registration becomes active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationStatus {
    /// Awaiting the desktop's binding response.
    Pending,
    /// Bound, with the actual desktop-provided trigger label where available.
    Active {
        /// Actual user-visible accelerator label.
        trigger: String,
    },
    /// Desktop refused or lost the binding; unregister before retrying.
    Failed {
        /// Native or portal failure details.
        message: String,
    },
}
/// A host event. Stable IDs refer to actions, not widget IDs or UI callbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// Successful registration, possibly after a Wayland confirmation dialog.
    Registered {
        /// Stable action ID.
        id: String,
        /// Actual desktop trigger label.
        trigger: String,
    },
    /// Triggered even when the app has no focused window.
    Pressed {
        /// Stable action ID.
        id: String,
        /// Wayland authorization token, if the portal supplied one.
        activation_token: Option<String>,
    },
    /// The trigger was released.
    Released {
        /// Stable action ID.
        id: String,
    },
    /// Asynchronous registration failure or session loss.
    Failed {
        /// Stable action ID.
        id: String,
        /// Native or portal failure details.
        message: String,
    },
}
impl HotkeyEvent {
    /// Application action identity associated with this event.
    pub fn id(&self) -> &str {
        match self {
            Self::Registered { id, .. }
            | Self::Pressed { id, .. }
            | Self::Released { id }
            | Self::Failed { id, .. } => id,
        }
    }
}
#[derive(Clone)]
struct Binding {
    id: String,
    hotkey: HotKey,
    description: String,
}
struct Entry<A> {
    binding: Binding,
    action: A,
    status: RegistrationStatus,
    generation: u64,
}
struct Delivery {
    event: HotkeyEvent,
    generation: u64,
}
#[derive(Clone)]
struct Sender {
    tx: mpsc::Sender<Delivery>,
    wake: Wake,
}
impl Sender {
    fn send(&self, generation: u64, event: HotkeyEvent) {
        if self.tx.send(Delivery { event, generation }).is_ok() {
            (self.wake)();
        }
    }
}
enum Backend {
    #[cfg(test)]
    Fake,
    Native(native::Native),
    #[cfg(target_os = "linux")]
    Portal(portal::Portal),
}
/// A main-thread, host-resident registry. Keep it outside reloadable application
/// state. Dropping it releases registrations; no action closures are retained.
///
/// Create on the native event-loop thread (mandatory on macOS/Windows). The
/// shell extension provides that lifecycle automatically. Wayland requires a
/// GlobalShortcuts-capable portal; it never falls back to XWayland grabs.
pub struct GlobalShortcuts<A> {
    backend: Backend,
    entries: BTreeMap<String, Entry<A>>,
    receiver: mpsc::Receiver<Delivery>,
    sender: Sender,
    generation: u64,
    _main_thread: PhantomData<Rc<()>>,
}
impl<A> GlobalShortcuts<A> {
    /// Create native resources and retain a callback that wakes an idle host.
    pub fn new(wake: Wake) -> Result<Self, Error> {
        let (tx, receiver) = mpsc::channel();
        let sender = Sender { tx, wake };
        #[cfg(target_os = "linux")]
        let backend = if std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("wayland"))
        {
            Backend::Portal(portal::Portal::new(sender.clone())?)
        } else {
            Backend::Native(native::Native::new()?)
        };
        #[cfg(not(target_os = "linux"))]
        let backend = Backend::Native(native::Native::new()?);
        Ok(Self {
            backend,
            entries: BTreeMap::new(),
            receiver,
            sender,
            generation: 0,
            _main_thread: PhantomData,
        })
    }
    /// Report whether native grabs or a desktop-mediated portal is in use.
    pub fn backend(&self) -> BackendKind {
        match self.backend {
            #[cfg(test)]
            Backend::Fake => BackendKind::Native,
            Backend::Native(_) => BackendKind::Native,
            #[cfg(target_os = "linux")]
            Backend::Portal(_) => BackendKind::Portal,
        }
    }
    /// Register a stable action ID, an accelerator (`CmdOrCtrl+Shift+K`) and a
    /// user-visible description. Duplicate IDs and equivalent chords are errors.
    /// On Wayland success here means queued; poll for Registered/Failed.
    pub fn register(
        &mut self,
        id: &str,
        accelerator: &str,
        description: &str,
        action: A,
    ) -> Result<(), Error> {
        let hotkey = accelerator
            .parse::<HotKey>()
            .map_err(|e| Error::new(e.to_string()))?;
        self.validate(id, hotkey)?;
        self.generation += 1;
        let generation = self.generation;
        let binding = Binding {
            id: id.into(),
            hotkey,
            description: description.into(),
        };
        match &mut self.backend {
            #[cfg(test)]
            Backend::Fake => self.sender.send(
                generation,
                HotkeyEvent::Registered {
                    id: id.into(),
                    trigger: accelerator.into(),
                },
            ),
            Backend::Native(native) => {
                native.register(binding.clone(), generation, self.sender.clone())?
            }
            #[cfg(target_os = "linux")]
            Backend::Portal(portal) => portal.register(binding.clone(), generation)?,
        }
        self.entries.insert(
            id.into(),
            Entry {
                binding,
                action,
                status: RegistrationStatus::Pending,
                generation,
            },
        );
        Ok(())
    }
    fn validate(&self, id: &str, hotkey: HotKey) -> Result<(), Error> {
        if id.is_empty() {
            return Err(Error::new("action ID must not be empty"));
        }
        if self.entries.contains_key(id)
            || self
                .entries
                .values()
                .any(|e| e.binding.hotkey.id() == hotkey.id())
        {
            return Err(Error::new(format!(
                "duplicate action ID or accelerator: {id}"
            )));
        }
        Ok(())
    }
    /// Whether this manager owns a queued or active registration (not a failed one).
    pub fn is_registered(&self, id: &str) -> bool {
        self.entries
            .get(id)
            .is_some_and(|e| !matches!(e.status, RegistrationStatus::Failed { .. }))
    }
    /// Current status. Drain events before inspecting asynchronous portal results.
    pub fn status(&self, id: &str) -> Option<&RegistrationStatus> {
        self.entries.get(id).map(|e| &e.status)
    }
    /// Action value for a live ID. UI/SDK resources remain in the host.
    pub fn action(&self, id: &str) -> Option<&A> {
        self.entries.get(id).map(|e| &e.action)
    }
    /// Unregister one ID. Unknown IDs are harmless. Already queued callbacks are ignored.
    pub fn unregister(&mut self, id: &str) -> Result<(), Error> {
        let Some(entry) = self.entries.get(id) else {
            return Ok(());
        };
        match &mut self.backend {
            #[cfg(test)]
            Backend::Fake => {}
            Backend::Native(native) => native.unregister(&entry.binding)?,
            #[cfg(target_os = "linux")]
            Backend::Portal(portal) => portal.unregister(id)?,
        }
        self.entries.remove(id);
        Ok(())
    }
    /// Attempt every release, retaining the first error and any failed native registrations.
    pub fn unregister_all(&mut self) -> Result<(), Error> {
        let ids: Vec<_> = self.entries.keys().cloned().collect();
        let mut first = None;
        for id in ids {
            if let Err(error) = self.unregister(&id) {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
    /// Drain one event on the UI thread. Obsolete deliveries after unregister or
    /// re-registration cannot trigger the new action.
    pub fn try_next(&mut self) -> Option<HotkeyEvent> {
        while let Ok(delivery) = self.receiver.try_recv() {
            let Some(entry) = self.entries.get_mut(delivery.event.id()) else {
                continue;
            };
            if entry.generation != delivery.generation {
                continue;
            }
            match &delivery.event {
                HotkeyEvent::Registered { trigger, .. } => {
                    entry.status = RegistrationStatus::Active {
                        trigger: trigger.clone(),
                    }
                }
                HotkeyEvent::Failed { message, .. } => {
                    entry.status = RegistrationStatus::Failed {
                        message: message.clone(),
                    }
                }
                _ => {}
            }
            return Some(delivery.event);
        }
        None
    }
}
impl<A> Drop for GlobalShortcuts<A> {
    fn drop(&mut self) {
        let _ = self.unregister_all();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn manager() -> GlobalShortcuts<u32> {
        let (tx, receiver) = mpsc::channel();
        GlobalShortcuts {
            backend: Backend::Fake,
            entries: BTreeMap::new(),
            receiver,
            sender: Sender {
                tx,
                wake: Arc::new(|| {}),
            },
            generation: 0,
            _main_thread: PhantomData,
        }
    }
    #[test]
    fn validates_equivalent_accelerators_without_replacing_actions() {
        let mut keys = manager();
        keys.register("palette", "Ctrl+Shift+K", "Open palette", 1)
            .unwrap();
        assert!(
            keys.register("other", "Shift+Control+k", "Other", 2)
                .is_err()
        );
        assert!(keys.register("palette", "Alt+Space", "Other", 2).is_err());
        assert!(keys.register("invalid", "not-a-key", "Invalid", 2).is_err());
        assert_eq!(keys.action("palette"), Some(&1));
        assert_eq!(keys.status("palette"), Some(&RegistrationStatus::Pending));
        assert!(matches!(
            keys.try_next(),
            Some(HotkeyEvent::Registered { .. })
        ));
        assert!(matches!(
            keys.status("palette"),
            Some(RegistrationStatus::Active { .. })
        ));
    }
    #[test]
    fn stale_events_cannot_activate_rebound_action_and_failures_are_visible() {
        let mut keys = manager();
        keys.register("palette", "Ctrl+K", "Open", 1).unwrap();
        let old = keys.generation;
        keys.unregister("palette").unwrap();
        keys.register("palette", "Ctrl+K", "Open", 2).unwrap();
        keys.sender.send(
            old,
            HotkeyEvent::Pressed {
                id: "palette".into(),
                activation_token: None,
            },
        );
        assert!(matches!(
            keys.try_next(),
            Some(HotkeyEvent::Registered { .. })
        ));
        assert_eq!(keys.try_next(), None);
        keys.sender.send(
            keys.generation,
            HotkeyEvent::Failed {
                id: "palette".into(),
                message: "desktop declined".into(),
            },
        );
        assert!(matches!(keys.try_next(), Some(HotkeyEvent::Failed { .. })));
        assert!(!keys.is_registered("palette"));
        keys.unregister_all().unwrap();
        assert_eq!(keys.action("palette"), None);
    }
}
