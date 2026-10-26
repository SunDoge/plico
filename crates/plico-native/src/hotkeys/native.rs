use super::*;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::{Mutex, Once, OnceLock};
struct Route {
    sender: Sender,
    generation: u64,
    id: String,
    pressed: bool,
}
fn routes() -> &'static Mutex<BTreeMap<u32, Route>> {
    static ROUTES: OnceLock<Mutex<BTreeMap<u32, Route>>> = OnceLock::new();
    ROUTES.get_or_init(Mutex::default)
}
fn deliver(event: GlobalHotKeyEvent) {
    let delivery = {
        let Ok(mut routes) = routes().lock() else {
            return;
        };
        let Some(route) = routes.get_mut(&event.id) else {
            return;
        };
        let pressed = event.state == HotKeyState::Pressed;
        if route.pressed == pressed {
            return;
        } // No repeated activation while held.
        route.pressed = pressed;
        (
            route.sender.clone(),
            route.generation,
            if pressed {
                HotkeyEvent::Pressed {
                    id: route.id.clone(),
                    activation_token: None,
                }
            } else {
                HotkeyEvent::Released {
                    id: route.id.clone(),
                }
            },
        )
    };
    delivery.0.send(delivery.1, delivery.2);
}
pub(super) struct Native {
    manager: GlobalHotKeyManager,
    owned: BTreeMap<u32, HotKey>,
}
impl Native {
    pub fn new() -> Result<Self, Error> {
        let manager = GlobalHotKeyManager::new().map_err(|e| Error::new(e.to_string()))?;
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| GlobalHotKeyEvent::set_event_handler(Some(deliver)));
        Ok(Self {
            manager,
            owned: BTreeMap::new(),
        })
    }
    pub fn register(
        &mut self,
        binding: Binding,
        generation: u64,
        sender: Sender,
    ) -> Result<(), Error> {
        let _ = &binding.description; // Native APIs have no user-visible description.
        {
            let mut routes = routes()
                .lock()
                .map_err(|_| Error::new("event router poisoned"))?;
            if routes.contains_key(&binding.hotkey.id()) {
                return Err(Error::new(
                    "accelerator already owned by another Plico manager",
                ));
            }
            routes.insert(
                binding.hotkey.id(),
                Route {
                    sender: sender.clone(),
                    generation,
                    id: binding.id.clone(),
                    pressed: false,
                },
            );
        }
        if let Err(error) = self.manager.register(binding.hotkey) {
            if let Ok(mut routes) = routes().lock() {
                routes.remove(&binding.hotkey.id());
            }
            return Err(Error::new(error.to_string()));
        }
        self.owned.insert(binding.hotkey.id(), binding.hotkey);
        sender.send(
            generation,
            HotkeyEvent::Registered {
                id: binding.id,
                trigger: binding.hotkey.to_string(),
            },
        );
        Ok(())
    }
    pub fn unregister(&mut self, binding: &Binding) -> Result<(), Error> {
        self.manager
            .unregister(binding.hotkey)
            .map_err(|e| Error::new(e.to_string()))?;
        if let Ok(mut routes) = routes().lock() {
            routes.remove(&binding.hotkey.id());
        }
        self.owned.remove(&binding.hotkey.id());
        Ok(())
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        for hotkey in self.owned.values() {
            let _ = self.manager.unregister(*hotkey);
            if let Ok(mut routes) = routes().lock() {
                routes.remove(&hotkey.id());
            }
        }
    }
}
