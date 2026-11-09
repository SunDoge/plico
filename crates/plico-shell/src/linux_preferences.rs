//! Portal preference snapshots, following MyGO linux/system.go's notifications.
//! One background connection per process; no D-Bus calls on the UI thread.
use plico_renderer::{DisplayPreferences, WakeCallback};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::OwnedValue,
};
const APPEARANCE: &str = "org.freedesktop.appearance";
const GNOME: &str = "org.gnome.desktop.interface";
const ACCESSIBILITY: &str = "org.gnome.desktop.a11y.interface";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";
type Settings = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Default)]
struct Values {
    motion: bool,
    gnome_motion: bool,
    contrast: bool,
    theme_contrast: bool,
    gnome_contrast: bool,
}
impl Values {
    fn snapshot(&self) -> DisplayPreferences {
        DisplayPreferences {
            reduce_motion: self.motion || self.gnome_motion,
            high_contrast: self.contrast || self.theme_contrast || self.gnome_contrast,
            reduce_transparency: false,
        }
    }
    fn apply(&mut self, namespace: &str, key: &str, value: &OwnedValue) {
        match (namespace, key) {
            (APPEARANCE, "reduced-motion") => {
                if let Ok(value) = u32::try_from(value) {
                    self.motion = value == 1;
                }
            }
            (APPEARANCE, "contrast") => {
                if let Ok(value) = u32::try_from(value) {
                    self.contrast = value == 1;
                }
            }
            (GNOME, "enable-animations") => {
                if let Ok(value) = bool::try_from(value) {
                    self.gnome_motion = !value;
                }
            }
            (GNOME, "gtk-theme") => {
                if let Ok(value) = <&str>::try_from(value) {
                    self.theme_contrast = value.to_lowercase().contains("highcontrast");
                }
            }
            (ACCESSIBILITY, "high-contrast") => {
                if let Ok(value) = bool::try_from(value) {
                    self.gnome_contrast = value;
                }
            }
            _ => {}
        }
    }
}
struct Subscription {
    dirty: AtomicBool,
    wake: WakeCallback,
}
impl Subscription {
    fn notify(&self) {
        if !self.dirty.swap(true, Ordering::AcqRel) {
            (self.wake)();
        }
    }
}
struct Service {
    values: Mutex<Values>,
    listeners: Mutex<Vec<Weak<Subscription>>>,
    connection: Mutex<Option<Connection>>,
}
impl Service {
    fn start(destination: String) -> Arc<Self> {
        let service = Arc::new(Self {
            values: Mutex::new(Values::default()),
            listeners: Mutex::new(Vec::new()),
            connection: Mutex::new(None),
        });
        let weak = Arc::downgrade(&service);
        if let Err(error) = std::thread::Builder::new()
            .name("plico-preferences".into())
            .spawn(move || {
                if let Err(error) = watch(weak.clone(), destination) {
                    // Missing portals are normal on headless/minimal desktops.
                    #[cfg(test)]
                    eprintln!("preference observer: {error}");
                    tracing::debug!(%error, "Linux preference portal unavailable");
                }
                let connection = weak
                    .upgrade()
                    .and_then(|owner| owner.connection.lock().unwrap().take());
                if let Some(connection) = connection {
                    let _ = connection.close();
                }
            })
        {
            tracing::warn!(%error, "could not start Linux preference observer");
        }
        service
    }
    fn update(&self, apply: impl FnOnce(&mut Values)) {
        let changed = {
            let mut values = self.values.lock().unwrap();
            let old = values.snapshot();
            apply(&mut values);
            old != values.snapshot()
        };
        if changed {
            let listeners = {
                let mut listeners = self.listeners.lock().unwrap();
                let live: Vec<_> = listeners.iter().filter_map(Weak::upgrade).collect();
                listeners.retain(|listener| listener.strong_count() > 0);
                live
            };
            // Do not hold a service lock while invoking even a wake callback.
            for listener in listeners {
                listener.notify();
            }
        }
    }
    fn subscribe(&self, listener: &Arc<Subscription>) {
        {
            let mut listeners = self.listeners.lock().unwrap();
            listeners.retain(|listener| listener.strong_count() > 0);
            listeners.push(Arc::downgrade(listener));
        }
        listener.notify();
    }
    fn snapshot(&self) -> DisplayPreferences {
        self.values.lock().unwrap().snapshot()
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        // Closing, rather than merely dropping one clone, unblocks the worker's
        // signal iterator. No join or wait for a portal reply on the UI thread.
        if let Some(connection) = self.connection.get_mut().unwrap().take() {
            let _ = connection.close();
        }
    }
}
fn shared_service() -> Arc<Service> {
    static SERVICE: OnceLock<Mutex<Weak<Service>>> = OnceLock::new();
    let mut slot = SERVICE
        .get_or_init(|| Mutex::new(Weak::new()))
        .lock()
        .unwrap();
    if let Some(service) = slot.upgrade() {
        return service;
    }
    let service = Service::start("org.freedesktop.portal.Desktop".into());
    *slot = Arc::downgrade(&service);
    service
}
fn watch(service: Weak<Service>, destination: String) -> zbus::Result<()> {
    let connection = Connection::session()?;
    if let Some(owner) = service.upgrade() {
        *owner.connection.lock().unwrap() = Some(connection.clone());
    } else {
        return connection.close();
    }
    // GetNameOwner in receive_signal does not activate a lazy portal. Ask the
    // bus to start it first, entirely on this worker. An already-running
    // non-activatable provider may reject activation, yet still serve settings.
    let _ = connection.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "StartServiceByName",
        &(destination.as_str(), 0u32),
    );
    let proxy = Proxy::new_owned(connection, destination, PATH, INTERFACE)?;
    // Subscribe before reading, so a setting changed during startup is not lost.
    let signals = proxy.receive_signal("SettingChanged")?;
    let namespaces: &[&str] = &[APPEARANCE, GNOME, ACCESSIBILITY];
    let initial: Settings = proxy.call("ReadAll", &(namespaces,))?;
    if let Some(owner) = service.upgrade() {
        owner.update(|values| {
            for (namespace, settings) in initial {
                for (key, value) in settings {
                    values.apply(&namespace, &key, &value);
                }
            }
        });
    } else {
        return Ok(());
    }
    for message in signals {
        let Some(owner) = service.upgrade() else {
            break;
        };
        if let Ok((namespace, key, value)) =
            message.body().deserialize::<(String, String, OwnedValue)>()
        {
            owner.update(|values| values.apply(&namespace, &key, &value));
        }
    }
    Ok(())
}
#[derive(Default)]
pub(crate) struct NativePreferences {
    value: DisplayPreferences,
    subscription: Option<Arc<Subscription>>,
    service: Option<Arc<Service>>,
}
impl NativePreferences {
    pub fn snapshot(&self) -> DisplayPreferences {
        self.value
    }
    pub fn observe(&mut self, wake: WakeCallback) {
        let service = shared_service();
        let subscription = Arc::new(Subscription {
            dirty: AtomicBool::new(false),
            wake,
        });
        service.subscribe(&subscription);
        self.subscription = Some(subscription);
        self.service = Some(service);
    }
    pub fn refresh(&mut self) -> bool {
        let Some(subscription) = &self.subscription else {
            return false;
        };
        if !subscription.dirty.swap(false, Ordering::AcqRel) {
            return false;
        }
        let next = self.service.as_ref().unwrap().snapshot();
        let changed = self.value != next;
        self.value = next;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::AtomicUsize,
        time::{Duration, Instant},
    };
    use zbus::zvariant::{Str, Value};

    #[test]
    fn desktop_motion_requests_survive_portal_no_preference_and_restore_cleanly() {
        let mut values = Values::default();
        values.apply(GNOME, "enable-animations", &false.into());
        assert!(values.snapshot().reduce_motion);
        values.apply(APPEARANCE, "reduced-motion", &0u32.into());
        assert!(
            values.snapshot().reduce_motion,
            "no preference must not undo a desktop request"
        );
        values.apply(GNOME, "enable-animations", &true.into());
        assert!(!values.snapshot().reduce_motion);
        values.apply(APPEARANCE, "reduced-motion", &1u32.into());
        values.apply(GNOME, "enable-animations", &true.into());
        assert!(values.snapshot().reduce_motion);
        values.apply(APPEARANCE, "reduced-motion", &0u32.into());
        assert_eq!(values.snapshot(), DisplayPreferences::default());
    }
    #[test]
    fn contrast_sources_combine_and_malformed_or_unrelated_settings_are_ignored() {
        let mut values = Values::default();
        values.apply(GNOME, "gtk-theme", &Str::from("HighContrastInverse").into());
        values.apply(APPEARANCE, "contrast", &0u32.into());
        assert!(values.snapshot().high_contrast);
        values.apply(GNOME, "gtk-theme", &Str::from("Adwaita").into());
        values.apply(ACCESSIBILITY, "high-contrast", &true.into());
        assert!(values.snapshot().high_contrast);
        values.apply(ACCESSIBILITY, "high-contrast", &false.into());
        values.apply(APPEARANCE, "contrast", &1u32.into());
        let expected = values.snapshot();
        values.apply(APPEARANCE, "contrast", &Str::from("invalid").into());
        values.apply("unknown.namespace", "contrast", &0u32.into());
        values.apply(APPEARANCE, "unknown-key", &0u32.into());
        assert_eq!(values.snapshot(), expected);
        values.apply(APPEARANCE, "contrast", &0u32.into());
        assert_eq!(values.snapshot(), DisplayPreferences::default());
    }
    #[test]
    fn changes_coalesce_per_window_and_only_effective_changes_wake() {
        let service = Arc::new(Service {
            values: Mutex::new(Values::default()),
            listeners: Mutex::new(Vec::new()),
            connection: Mutex::new(None),
        });
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        let subscription = Arc::new(Subscription {
            dirty: AtomicBool::new(false),
            wake: Arc::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            }),
        });
        service.subscribe(&subscription);
        let mut preferences = NativePreferences {
            value: Default::default(),
            subscription: Some(subscription),
            service: Some(service.clone()),
        };
        assert!(!preferences.refresh());
        service.update(|values| values.apply(APPEARANCE, "contrast", &1u32.into()));
        service.update(|values| values.apply(APPEARANCE, "reduced-motion", &1u32.into()));
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            2,
            "one initial wake and one coalesced change"
        );
        assert!(preferences.refresh());
        assert!(preferences.snapshot().high_contrast && preferences.snapshot().reduce_motion);
        assert!(!preferences.refresh(), "idle UI does not poll the bus");
        service.update(|values| values.apply(APPEARANCE, "contrast", &1u32.into()));
        assert_eq!(wakes.load(Ordering::SeqCst), 2);
        drop(preferences);
        service.update(|values| values.apply(APPEARANCE, "contrast", &0u32.into()));
        assert!(service.listeners.lock().unwrap().is_empty());
    }

    struct MockSettings;
    #[zbus::interface(name = "org.freedesktop.portal.Settings")]
    impl MockSettings {
        fn read_all(&self, _namespaces: Vec<String>) -> Settings {
            HashMap::from([(
                APPEARANCE.into(),
                HashMap::from([
                    ("contrast".into(), 1u32.into()),
                    ("reduced-motion".into(), 1u32.into()),
                ]),
            )])
        }
    }
    fn until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "preference observer did not make progress"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "requires a session bus; run with dbus-run-session"]
    fn portal_initial_snapshot_signals_and_last_subscriber_shutdown_work_over_dbus() {
        let destination = format!("org.plico.TestPreferences.p{}", std::process::id());
        let server = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name(destination.as_str())
            .unwrap()
            .serve_at(PATH, MockSettings)
            .unwrap()
            .build()
            .unwrap();
        let service = Service::start(destination);
        until(|| service.snapshot().reduce_motion && service.snapshot().high_contrast);
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        let subscription = Arc::new(Subscription {
            dirty: AtomicBool::new(false),
            wake: Arc::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            }),
        });
        service.subscribe(&subscription);
        subscription.dirty.store(false, Ordering::Release);
        let connection = service.connection.lock().unwrap().as_ref().unwrap().clone();
        for key in ["contrast", "reduced-motion"] {
            server
                .emit_signal(
                    None::<&str>,
                    PATH,
                    INTERFACE,
                    "SettingChanged",
                    &(APPEARANCE, key, Value::U32(0)),
                )
                .unwrap();
        }
        until(|| service.snapshot() == DisplayPreferences::default());
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            2,
            "signal changes coalesce until consumed"
        );
        let weak = Arc::downgrade(&service);
        drop(service);
        until(|| weak.upgrade().is_none());
        // Observe actual connection termination, not just a dropped owner: this
        // also proves the blocking signal iterator can be released at shutdown.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            connection.closed();
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("observer connection closed");
    }
}
