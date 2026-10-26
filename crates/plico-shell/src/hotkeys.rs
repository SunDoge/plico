//! Optional system-wide hotkey integration with the existing native event loop.
use crate::{ExtensionContext, ShellExtension, WakeCallback};
pub use plico_native::hotkeys::{
    BackendKind, Error, GlobalShortcuts, HotkeyEvent, RegistrationStatus,
};
/// One host-resident binding. Actions are never serialized into reloadable UI state.
pub struct GlobalHotkeyBinding<A> {
    /// Stable application action ID; used by the Wayland desktop to remember preferences.
    pub id: String,
    /// Accelerator such as `CmdOrCtrl+Shift+K`.
    pub accelerator: String,
    /// User-visible explanation shown in desktop authorization dialogs.
    pub description: String,
    /// Ordinary host action value.
    pub action: A,
}
/// Shell-owned registration, wake delivery and shutdown. The handler runs on the
/// event-loop thread, even when no application window is focused. Inspect the
/// event: Registered/Released/Failed must not be treated as activation.
pub struct GlobalHotkeyExtension<A, F> {
    bindings: Vec<GlobalHotkeyBinding<A>>,
    manager: Option<GlobalShortcuts<A>>,
    handler: F,
}
impl<A, F> GlobalHotkeyExtension<A, F> {
    /// Retain configuration without registering keys before the native loop exists.
    pub fn new(bindings: Vec<GlobalHotkeyBinding<A>>, handler: F) -> Self {
        Self {
            bindings,
            manager: None,
            handler,
        }
    }
}
impl<A, F> ShellExtension for GlobalHotkeyExtension<A, F>
where
    F: FnMut(&HotkeyEvent, &A, &mut ExtensionContext<'_>),
{
    fn initialize(&mut self, wake: WakeCallback) -> Result<(), String> {
        let mut manager = GlobalShortcuts::new(wake).map_err(|e| e.to_string())?;
        for binding in self.bindings.drain(..) {
            manager
                .register(
                    &binding.id,
                    &binding.accelerator,
                    &binding.description,
                    binding.action,
                )
                .map_err(|e| e.to_string())?;
        }
        self.manager = Some(manager);
        Ok(())
    }
    fn poll(&mut self, context: &mut ExtensionContext<'_>) {
        let Some(manager) = self.manager.as_mut() else {
            return;
        };
        while let Some(event) = manager.try_next() {
            if let Some(action) = manager.action(event.id()) {
                (self.handler)(&event, action, context);
            }
        }
    }
    fn shutdown(&mut self, _: &mut ExtensionContext<'_>) {
        self.manager.take();
    }
}
