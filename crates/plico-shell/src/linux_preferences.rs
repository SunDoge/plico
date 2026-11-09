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
use zbus::{Connection, zvariant::OwnedValue};
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
        if let Some(change) = Change::parse(namespace, key, value) {
            self.set(change);
        }
    }
    fn set(&mut self, change: Change) {
        match change.key {
            0 => self.motion = change.value,
            1 => self.gnome_motion = change.value,
            2 => self.contrast = change.value,
            3 => self.theme_contrast = change.value,
            4 => self.gnome_contrast = change.value,
            _ => unreachable!(),
        }
    }
    fn get(&self, key: usize) -> bool {
        match key {
            0 => self.motion,
            1 => self.gnome_motion,
            2 => self.contrast,
            3 => self.theme_contrast,
            4 => self.gnome_contrast,
            _ => unreachable!(),
        }
    }
}
struct Change {
    key: usize,
    value: bool,
}
impl Change {
    fn parse(namespace: &str, key: &str, value: &OwnedValue) -> Option<Self> {
        let (key, value) = match (namespace, key) {
            (APPEARANCE, "reduced-motion") => (0, u32::try_from(value).ok()? == 1),
            (GNOME, "enable-animations") => (1, !bool::try_from(value).ok()?),
            (APPEARANCE, "contrast") => (2, u32::try_from(value).ok()? == 1),
            (GNOME, "gtk-theme") => (
                3,
                <&str>::try_from(value)
                    .ok()?
                    .to_lowercase()
                    .contains("highcontrast"),
            ),
            (ACCESSIBILITY, "high-contrast") => (4, bool::try_from(value).ok()?),
            _ => return None,
        };
        Some(Self { key, value })
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
    #[cfg(test)]
    observer_ready: AtomicBool,
    #[cfg(test)]
    settings_seen: std::sync::atomic::AtomicUsize,
}
impl Service {
    fn start(destination: String) -> Arc<Self> {
        let service = Arc::new(Self {
            values: Mutex::new(Values::default()),
            listeners: Mutex::new(Vec::new()),
            connection: Mutex::new(None),
            #[cfg(test)]
            observer_ready: AtomicBool::new(false),
            #[cfg(test)]
            settings_seen: std::sync::atomic::AtomicUsize::new(0),
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
                    let _ = futures_lite::future::block_on(connection.close());
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
            let _ = futures_lite::future::block_on(connection.close());
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
    // Build the async-io connection inside our own worker, even when another
    // application dependency also enables zbus's Tokio feature.
    futures_lite::future::block_on(async {
        let connection = zbus::Connection::session().await?;
        if let Some(owner) = service.upgrade() {
            *owner.connection.lock().unwrap() = Some(connection.clone());
        } else {
            return connection.close().await;
        }
        watch_async(service, connection, destination).await
    })
}
fn read_settings(
    connection: zbus::Connection,
    destination: String,
) -> futures_util::future::BoxFuture<'static, zbus::Result<(Settings, String)>> {
    use futures_util::FutureExt;
    async move {
        let namespaces: &[&str] = &[APPEARANCE, GNOME, ACCESSIBILITY];
        // Ordinary calls can activate a lazy portal. An absent service is a
        // recoverable read failure; the owner watcher remains subscribed.
        let reply = connection
            .call_method(
                Some(destination.as_str()),
                PATH,
                Some(INTERFACE),
                "ReadAll",
                &(namespaces,),
            )
            .await?;
        let sender = reply
            .header()
            .sender()
            .map(|name| name.to_string())
            .unwrap_or_default();
        Ok((reply.body().deserialize()?, sender))
    }
    .boxed()
}
async fn watch_async(
    service: Weak<Service>,
    connection: zbus::Connection,
    destination: String,
) -> zbus::Result<()> {
    use futures_util::{FutureExt, StreamExt};
    use zbus::{MatchRule, MessageStream, message::Type};
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    // Match rules work even when the portal has no owner, unlike a proxy's
    // receive_signal, which first resolves an existing service name.
    let mut settings = MessageStream::for_match_rule(
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(destination.as_str())?
            .path(PATH)?
            .interface(INTERFACE)?
            .member("SettingChanged")?
            .build(),
        &connection,
        Some(64),
    )
    .await?;
    let mut owners = MessageStream::for_match_rule(
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg(0, destination.as_str())?
            .build(),
        &connection,
        Some(8),
    )
    .await?;
    // Resolve only after subscribing: an owner switch between resolving the
    // name and adding the match rule must not leave a stale sender forever.
    let mut current_owner = bus
        .get_name_owner(destination.as_str().try_into()?)
        .await
        .ok()
        .map(|name| name.to_string());
    let mut versions = [0u64; 5];
    let mut read_versions = versions;
    let mut reload = read_settings(connection.clone(), destination.clone()).fuse();
    #[cfg(test)]
    if let Some(owner) = service.upgrade() {
        owner.observer_ready.store(true, Ordering::Release);
    }
    loop {
        // Continue draining both streams while ReadAll is pending. A burst of
        // settings cannot fill a signal queue and block its own method reply.
        futures_util::select! {
            message = settings.next().fuse() => {
                let Some(message) = message else { break; };
                let message = message?;
                let sender = message.header().sender().map(|name| name.to_string());
                if sender != current_owner { continue; }
                if let Ok((namespace, key, value)) = message.body().deserialize::<(String, String, OwnedValue)>()
                    && let Some(change) = Change::parse(&namespace, &key, &value)
                {
                    versions[change.key] = versions[change.key].saturating_add(1);
                    let Some(owner) = service.upgrade() else { break; };
                    owner.update(|values| values.set(change));
                    #[cfg(test)]
                    owner.settings_seen.fetch_add(1, Ordering::SeqCst);
                }
            },
            message = owners.next().fuse() => {
                let Some(message) = message else { break; };
                let message = message?;
                if let Ok((name, _, new_owner)) = message.body().deserialize::<(String, String, String)>() {
                    if name != destination { continue; }
                    current_owner = (!new_owner.is_empty()).then_some(new_owner);
                    versions = [0; 5];
                    read_versions = versions;
                    reload = if current_owner.is_some() {
                        read_settings(connection.clone(), destination.clone()).fuse()
                    } else {
                        // Hold the last valid preference while the backend is
                        // absent. A new owner supplies a fresh complete snapshot.
                        futures_util::future::pending().boxed().fuse()
                    };
                }
            },
            result = reload => {
                if let Ok((settings, sender)) = result {
                    if current_owner.as_ref().is_some_and(|owner| *owner != sender) { continue; }
                    current_owner = Some(sender);
                    let mut snapshot = Values::default();
                    for (namespace, settings) in settings {
                        for (key, value) in settings { snapshot.apply(&namespace, &key, &value); }
                    }
                    let Some(owner) = service.upgrade() else { break; };
                    owner.update(|values| {
                        for key in 0..5 {
                            // Never overwrite a newer signal with an earlier
                            // ReadAll response. Unchanged/missing keys refresh.
                            if versions[key] == read_versions[key] {
                                values.set(Change { key, value: snapshot.get(key) });
                            }
                        }
                    });
                }
            },
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
            #[cfg(test)]
            observer_ready: AtomicBool::new(false),
            #[cfg(test)]
            settings_seen: std::sync::atomic::AtomicUsize::new(0),
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
            futures_lite::future::block_on(connection.closed());
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("observer connection closed");
    }

    struct EmptySettings;
    #[zbus::interface(name = "org.freedesktop.portal.Settings")]
    impl EmptySettings {
        fn read_all(&self, _namespaces: Vec<String>) -> Settings {
            HashMap::new()
        }
    }
    #[test]
    #[ignore = "requires a session bus; run with dbus-run-session"]
    fn portal_can_appear_late_and_restart_without_losing_the_observer() {
        let destination = format!("org.plico.TestPreferences.recovery.p{}", std::process::id());
        let service = Service::start(destination.clone());
        until(|| service.observer_ready.load(Ordering::Acquire));
        assert_eq!(service.snapshot(), DisplayPreferences::default());
        let first = zbus::blocking::connection::Builder::session()
            .unwrap()
            .serve_at(PATH, MockSettings)
            .unwrap()
            .build()
            .unwrap();
        first.request_name(destination.as_str()).unwrap();
        until(|| service.snapshot().reduce_motion && service.snapshot().high_contrast);
        first.release_name(destination.as_str()).unwrap();
        let second = zbus::blocking::connection::Builder::session()
            .unwrap()
            .serve_at(PATH, EmptySettings)
            .unwrap()
            .build()
            .unwrap();
        second.request_name(destination.as_str()).unwrap();
        until(|| service.snapshot() == DisplayPreferences::default());
        second
            .emit_signal(
                None::<&str>,
                PATH,
                INTERFACE,
                "SettingChanged",
                &(APPEARANCE, "reduced-motion", Value::U32(1)),
            )
            .unwrap();
        until(|| service.snapshot().reduce_motion);
        assert!(
            !service.snapshot().high_contrast,
            "old backend keys were cleared"
        );
    }

    #[derive(Default)]
    struct Gate {
        entered: AtomicBool,
        open: AtomicBool,
        waker: Mutex<Option<std::task::Waker>>,
    }
    impl Gate {
        async fn wait(&self) {
            self.entered.store(true, Ordering::Release);
            futures_util::future::poll_fn(|cx| {
                let mut waker = self.waker.lock().unwrap();
                if self.open.load(Ordering::Acquire) {
                    std::task::Poll::Ready(())
                } else {
                    *waker = Some(cx.waker().clone());
                    std::task::Poll::Pending
                }
            })
            .await
        }
        fn release(&self) {
            self.open.store(true, Ordering::Release);
            if let Some(waker) = self.waker.lock().unwrap().take() {
                waker.wake();
            }
        }
    }
    struct DelayedSettings(Arc<Gate>);
    #[zbus::interface(name = "org.freedesktop.portal.Settings")]
    impl DelayedSettings {
        async fn read_all(&self, _namespaces: Vec<String>) -> Settings {
            self.0.wait().await;
            HashMap::from([(
                APPEARANCE.into(),
                HashMap::from([
                    ("contrast".into(), 1u32.into()),
                    ("reduced-motion".into(), 0u32.into()),
                ]),
            )])
        }
    }
    #[test]
    #[ignore = "requires a session bus; run with dbus-run-session"]
    fn pending_snapshot_drains_signal_bursts_and_cannot_overwrite_newer_settings() {
        let destination = format!("org.plico.TestPreferences.burst.p{}", std::process::id());
        let gate = Arc::new(Gate::default());
        let server = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name(destination.as_str())
            .unwrap()
            .serve_at(PATH, DelayedSettings(gate.clone()))
            .unwrap()
            .build()
            .unwrap();
        let service = Service::start(destination);
        until(|| gate.entered.load(Ordering::Acquire));
        for index in 0..128u32 {
            server
                .emit_signal(
                    None::<&str>,
                    PATH,
                    INTERFACE,
                    "SettingChanged",
                    &(APPEARANCE, "reduced-motion", Value::U32(index % 2)),
                )
                .unwrap();
        }
        until(|| service.settings_seen.load(Ordering::SeqCst) == 128);
        assert!(service.snapshot().reduce_motion);
        gate.release();
        until(|| service.snapshot().high_contrast);
        assert!(
            service.snapshot().reduce_motion,
            "stale ReadAll must not undo the newer signal"
        );
    }
}
