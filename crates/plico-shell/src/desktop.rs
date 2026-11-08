//! Opt-in native tray and actionable notifications, owned by the shell event loop.
//!
//! Applications supply branding, menu semantics, notification contents and persistence.
//! No native callback receives UI or application-state references. A process can own
//! one integration because the native menu library has process-global callbacks.
use crate::{
    ExtensionContext, NotificationRequest, ShellExtension, WakeCallback, WindowResourceKey,
};
use snafu::Snafu;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

/// Operation associated with an ordinary native menu item.
#[derive(Clone, Copy, Debug)]
pub enum MenuAction {
    /// Show, restore and focus the configured logical window.
    ShowWindow,
    /// Terminate the event loop; application host destruction performs app cleanup.
    Exit,
    /// Deliver the item's ID to the application without interpreting it.
    Emit,
}
/// Native menu declaration, independent of any application's domain.
#[derive(Clone, Debug)]
pub enum TrayItem {
    /// Ordinary action item.
    Action {
        /// Application-owned stable ID.
        id: String,
        /// Visible native label.
        label: String,
        /// Native or application operation.
        action: MenuAction,
    },
    /// Checkbox whose changes are reported to the application.
    Toggle {
        /// Application-owned stable ID.
        id: String,
        /// Visible native label.
        label: String,
        /// Initial checkbox value.
        checked: bool,
    },
    /// Native separator.
    Separator,
}
/// Application-owned icon pixels; macOS can render the alpha as a menu-bar template.
#[derive(Clone, Debug)]
pub struct TrayImage {
    /// Straight-alpha RGBA pixels, exactly width * height * 4 bytes.
    pub rgba: Vec<u8>,
    /// Width in pixels (1–256).
    pub width: u32,
    /// Height in pixels (1–256).
    pub height: u32,
    /// Request automatic light/dark menu-bar recoloring on macOS.
    pub macos_template: bool,
}
/// Initial native integration configuration. Preferences remain app-owned.
#[derive(Clone, Debug)]
pub struct DesktopOptions {
    /// Stable process/app identity, also used for Windows notifications.
    pub app_id: String,
    /// Human-readable tray tooltip and notification application name.
    pub title: String,
    /// Window restored by tray and notification activation.
    pub window: WindowResourceKey,
    /// App branding; the framework supplies no default logo.
    pub image: TrayImage,
    /// Native menu declaration; IDs must be unique and nonempty.
    pub menu: Vec<TrayItem>,
    /// Initially hide on close when a tray host is available.
    pub close_to_tray: bool,
    /// Initially allow new notifications.
    pub notifications_enabled: bool,
}
/// Domain-neutral messages to be drained by a host even when its window is hidden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopEvent {
    /// An action or toggle was selected. Built-in show/exit actions also emit this.
    Menu {
        /// Stable application ID.
        id: String,
        /// New value for toggles, None for actions.
        checked: Option<bool>,
    },
    /// Native tray initialization failed; closing remains a normal exit.
    TrayUnavailable,
    /// Notification delivery or response observation failed.
    NotificationFailed(u64),
    /// The user activated a notification; its window was restored first.
    NotificationActivated(u64),
}
/// Failure to construct or enqueue a desktop service operation.
#[derive(Debug, Snafu)]
pub enum DesktopError {
    /// Malformed application-supplied options or notification.
    #[snafu(display("Invalid desktop configuration: {reason}"))]
    Invalid {
        /// Description without application data.
        reason: &'static str,
    },
    /// One notification observer is already active.
    #[snafu(display("Desktop notification service is busy"))]
    Busy,
    /// The integration is shut down or notifications are disabled.
    #[snafu(display("Desktop notification service is unavailable"))]
    Unavailable,
}
struct Shared {
    close: AtomicBool,
    enabled: AtomicBool,
    busy: AtomicBool,
    stopped: AtomicBool,
    wake: Mutex<Option<WakeCallback>>,
}
impl Shared {
    fn wake(&self) {
        if let Some(wake) = self.wake.lock().ok().and_then(|slot| slot.clone()) {
            wake();
        }
    }
}
enum Command {
    Menu(String),
    Open,
    Notify(u64, NotificationRequest),
    Activated(u64),
}
/// Cloneable, thread-safe notification and native-policy handle.
#[derive(Clone)]
pub struct DesktopHandle {
    shared: Arc<Shared>,
    tx: mpsc::Sender<Command>,
}
impl DesktopHandle {
    /// Update close policy immediately, including while the window is hidden.
    pub fn set_close_to_tray(&self, enabled: bool) {
        self.shared.close.store(enabled, Ordering::Relaxed);
        self.shared.wake();
    }
    /// Permit or suppress new notifications. Already delivered notifications remain native-owned.
    pub fn set_notifications_enabled(&self, enabled: bool) {
        self.shared.enabled.store(enabled, Ordering::Relaxed);
    }
    /// Queue one notification, with a caller-owned correlation ID and a 30-second
    /// native expiry hint. At most one observer runs, even if the OS ignores expiry.
    /// Activation restores the configured window and reports the ID; it never
    /// executes an application action. Callers must exclude secrets from contents.
    pub fn notify(&self, id: u64, request: NotificationRequest) -> Result<(), DesktopError> {
        if request.title.trim().is_empty() {
            return Err(DesktopError::Invalid {
                reason: "notification title is empty",
            });
        }
        if self.shared.stopped.load(Ordering::Acquire)
            || !self.shared.enabled.load(Ordering::Relaxed)
        {
            return Err(DesktopError::Unavailable);
        }
        if self.shared.busy.swap(true, Ordering::AcqRel) {
            return Err(DesktopError::Busy);
        }
        if self.tx.send(Command::Notify(id, request)).is_err() {
            self.shared.busy.store(false, Ordering::Release);
            return Err(DesktopError::Unavailable);
        }
        self.shared.wake();
        Ok(())
    }
}
/// Host-side event receiver and service handle. No UI state is retained by the service.
pub struct DesktopClient {
    events: mpsc::Receiver<DesktopEvent>,
    handle: DesktopHandle,
}
impl DesktopClient {
    /// Drain a native result without blocking the UI thread.
    pub fn try_recv(&self) -> Option<DesktopEvent> {
        self.events.try_recv().ok()
    }
    /// Clone a service handle for host-owned asynchronous producers.
    pub fn handle(&self) -> DesktopHandle {
        self.handle.clone()
    }
}
static NATIVE_OWNER: AtomicBool = AtomicBool::new(false);
/// Native resources, constructed on the event-loop thread by ShellExtension.
pub struct DesktopExtension {
    options: DesktopOptions,
    client: DesktopHandle,
    commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<DesktopEvent>,
    icon: Option<TrayIcon>,
    toggles: HashMap<String, CheckMenuItem>,
    available: Arc<AtomicBool>,
    watcher_stop: Option<mpsc::Sender<()>>,
    hidden: bool,
    owns_callbacks: bool,
}
/// Validate a declaration and construct channels; no native resources are created
/// until the extension is installed in a running shell.
pub fn integration(
    options: DesktopOptions,
) -> Result<(DesktopClient, DesktopExtension), DesktopError> {
    validate(&options)?;
    let (tx, commands) = mpsc::channel();
    let (events, receiver) = mpsc::channel();
    let handle = DesktopHandle {
        tx,
        shared: Arc::new(Shared {
            close: AtomicBool::new(options.close_to_tray),
            enabled: AtomicBool::new(options.notifications_enabled),
            busy: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            wake: Mutex::new(None),
        }),
    };
    let extension = DesktopExtension {
        options,
        client: handle.clone(),
        commands,
        events,
        icon: None,
        toggles: HashMap::new(),
        available: Arc::new(AtomicBool::new(!cfg!(target_os = "linux"))),
        watcher_stop: None,
        hidden: false,
        owns_callbacks: false,
    };
    Ok((
        DesktopClient {
            events: receiver,
            handle,
        },
        extension,
    ))
}
fn validate(options: &DesktopOptions) -> Result<(), DesktopError> {
    let image = &options.image;
    let mut ids = HashSet::new();
    let reason = if options.app_id.trim().is_empty() || options.title.trim().is_empty() {
        Some("app identity is empty")
    } else if image.width == 0
        || image.height == 0
        || image.width > 256
        || image.height > 256
        || image.rgba.len() != image.width as usize * image.height as usize * 4
    {
        Some("tray image dimensions are invalid")
    } else if options.menu.iter().any(|item| match item {
        TrayItem::Action { id, label, .. } | TrayItem::Toggle { id, label, .. } => {
            id.is_empty() || label.is_empty() || !ids.insert(id)
        }
        TrayItem::Separator => false,
    }) {
        Some("menu IDs or labels are empty or duplicated")
    } else {
        None
    };
    if let Some(reason) = reason {
        Err(DesktopError::Invalid { reason })
    } else {
        Ok(())
    }
}
impl DesktopExtension {
    fn emit(&self, event: DesktopEvent) {
        let _ = self.events.send(event);
        self.client.shared.wake();
    }
    fn install_tray(&mut self) -> Result<(), String> {
        let menu = Menu::new();
        for item in &self.options.menu {
            match item {
                TrayItem::Action { id, label, .. } => menu
                    .append(&MenuItem::with_id(id.clone(), label, true, None))
                    .map_err(|e| e.to_string())?,
                TrayItem::Toggle { id, label, checked } => {
                    let control = CheckMenuItem::with_id(id.clone(), label, true, *checked, None);
                    menu.append(&control).map_err(|e| e.to_string())?;
                    self.toggles.insert(id.clone(), control);
                }
                TrayItem::Separator => menu
                    .append(&PredefinedMenuItem::separator())
                    .map_err(|e| e.to_string())?,
            }
        }
        let image = &self.options.image;
        let pixels = tray_icon::Icon::from_rgba(image.rgba.clone(), image.width, image.height)
            .map_err(|e| e.to_string())?;
        let builder = TrayIconBuilder::new()
            .with_id(self.options.app_id.clone())
            .with_tooltip(&self.options.title)
            .with_menu(Box::new(menu));
        #[cfg(target_os = "macos")]
        let builder = if image.macos_template {
            builder.with_icon_templated(pixels)
        } else {
            builder.with_icon(pixels)
        };
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(pixels);
        self.icon = Some(builder.build().map_err(|e| e.to_string())?);
        let handle = self.client.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = handle.tx.send(Command::Menu(event.id.0));
            handle.shared.wake();
        }));
        let handle = self.client.clone();
        let id = self.options.app_id.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(event, TrayIconEvent::Click {id: ref event_id, button: MouseButton::Left, button_state: MouseButtonState::Up, ..} if event_id.0 == id)
            {
                let _ = handle.tx.send(Command::Open);
                handle.shared.wake();
            }
        }));
        Ok(())
    }
    fn observe_notification(&self, id: u64, request: NotificationRequest) {
        let shared = self.client.shared.clone();
        let tx = self.client.tx.clone();
        let events = self.events.clone();
        let title = self.options.title.clone();
        let identity = self.options.app_id.clone();
        let spawned = std::thread::Builder::new().name("plico-notification".into()).spawn(move || {
            struct Release(Arc<Shared>);
            impl Drop for Release { fn drop(&mut self) { self.0.busy.store(false, Ordering::Release); } }
            let _release = Release(shared.clone());
            if shared.stopped.load(Ordering::Acquire) || !shared.enabled.load(Ordering::Relaxed) { return; }
            let result = crate::system::notification_builder(&title, &request).and_then(|mut notice| {
                notice.timeout(30_000);
                #[cfg(target_os = "windows")] notice.app_id(&identity);
                #[cfg(not(target_os = "windows"))] let _ = identity;
                #[cfg(any(target_os = "linux", target_os = "windows"))] notice.action("default", &title);
                notice.show().and_then(|handle| handle.wait_for_response(|response: &notify_rust::NotificationResponse| {
                    if matches!(response, notify_rust::NotificationResponse::Default)
                        || matches!(response, notify_rust::NotificationResponse::Action(action) if action == "default") {
                        let _ = tx.send(Command::Activated(id)); shared.wake();
                    }
                })).map_err(|error| error.to_string())
            });
            if result.is_err() { let _ = events.send(DesktopEvent::NotificationFailed(id)); shared.wake(); }
        });
        if spawned.is_err() {
            self.client.shared.busy.store(false, Ordering::Release);
            self.emit(DesktopEvent::NotificationFailed(id));
        }
    }
    fn restore(&mut self, context: &mut ExtensionContext<'_>) {
        if context.show_window(self.options.window) {
            self.hidden = false;
        }
    }
}
impl ShellExtension for DesktopExtension {
    fn initialize(&mut self, wake: WakeCallback) -> Result<(), String> {
        *self
            .client
            .shared
            .wake
            .lock()
            .map_err(|_| "desktop wake state poisoned")? = Some(wake);
        if NATIVE_OWNER.swap(true, Ordering::AcqRel) {
            self.available.store(false, Ordering::Relaxed);
            self.emit(DesktopEvent::TrayUnavailable);
            return Ok(());
        }
        self.owns_callbacks = true;
        if self.install_tray().is_err() {
            self.available.store(false, Ordering::Relaxed);
            self.emit(DesktopEvent::TrayUnavailable);
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            let (stop, receiver) = mpsc::channel();
            self.watcher_stop = Some(stop);
            let available = self.available.clone();
            let shared = self.client.shared.clone();
            std::thread::spawn(move || {
                loop {
                    let present = status_notifier_host();
                    if available.swap(present, Ordering::Relaxed) != present {
                        shared.wake();
                    }
                    if !matches!(
                        receiver.recv_timeout(std::time::Duration::from_secs(3)),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        break;
                    }
                }
            });
        }
        Ok(())
    }
    fn poll(&mut self, context: &mut ExtensionContext<'_>) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Open => self.restore(context),
                Command::Activated(id) => {
                    self.restore(context);
                    self.emit(DesktopEvent::NotificationActivated(id));
                }
                Command::Notify(id, request) => self.observe_notification(id, request),
                Command::Menu(id) => {
                    let Some(item) = self.options.menu.iter_mut().find(|item| match item {
                        TrayItem::Action { id: key, .. } | TrayItem::Toggle { id: key, .. } => {
                            *key == id
                        }
                        _ => false,
                    }) else {
                        continue;
                    };
                    let (action, checked) = match item {
                        TrayItem::Action { action, .. } => (*action, None),
                        TrayItem::Toggle { checked, .. } => {
                            *checked = !*checked;
                            if let Some(item) = self.toggles.get(&id) {
                                item.set_checked(*checked);
                            }
                            (MenuAction::Emit, Some(*checked))
                        }
                        TrayItem::Separator => unreachable!(),
                    };
                    self.emit(DesktopEvent::Menu { id, checked });
                    match action {
                        MenuAction::ShowWindow => self.restore(context),
                        MenuAction::Exit => context.exit(),
                        MenuAction::Emit => {}
                    }
                }
            }
        }
        if self.hidden
            && (!self.available.load(Ordering::Relaxed)
                || !self.client.shared.close.load(Ordering::Relaxed))
        {
            self.restore(context);
        }
    }
    fn close_requested(
        &mut self,
        window: WindowResourceKey,
        context: &mut ExtensionContext<'_>,
    ) -> bool {
        let hide = window == self.options.window
            && self.icon.is_some()
            && self.available.load(Ordering::Relaxed)
            && self.client.shared.close.load(Ordering::Relaxed)
            && context.hide_window(window);
        self.hidden |= hide;
        hide
    }
    fn shutdown(&mut self, _: &mut ExtensionContext<'_>) {
        self.release();
    }
}
impl DesktopExtension {
    fn release(&mut self) {
        self.client.shared.stopped.store(true, Ordering::Release);
        if let Some(stop) = self.watcher_stop.take() {
            let _ = stop.send(());
        }
        self.icon.take();
        if self.owns_callbacks {
            MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
            TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
            NATIVE_OWNER.store(false, Ordering::Release);
            self.owns_callbacks = false;
        }
    }
}
impl Drop for DesktopExtension {
    fn drop(&mut self) {
        self.release();
    }
}
#[cfg(target_os = "linux")]
fn status_notifier_host() -> bool {
    (|| -> zbus::Result<bool> {
        let bus = zbus::blocking::Connection::session()?;
        let proxy = zbus::blocking::Proxy::new(
            &bus,
            "org.kde.StatusNotifierWatcher",
            "/StatusNotifierWatcher",
            "org.kde.StatusNotifierWatcher",
        )?;
        proxy.get_property("IsStatusNotifierHostRegistered")
    })()
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options() -> DesktopOptions {
        DesktopOptions {
            app_id: "plico.test".into(),
            title: "Test".into(),
            window: crate::initial_window_resource_key(0),
            image: TrayImage {
                rgba: vec![0, 0, 0, 255],
                width: 1,
                height: 1,
                macos_template: true,
            },
            menu: vec![TrayItem::Action {
                id: "open".into(),
                label: "Open".into(),
                action: MenuAction::ShowWindow,
            }],
            close_to_tray: true,
            notifications_enabled: true,
        }
    }
    fn notification() -> NotificationRequest {
        NotificationRequest {
            title: "Synthetic notification".into(),
            body: None,
            icon: None,
            silent: true,
        }
    }
    #[test]
    fn invalid_images_and_ambiguous_menu_ids_are_rejected_before_native_creation() {
        let mut config = options();
        config.image.width = 0;
        assert!(integration(config).is_err());
        let mut config = options();
        config.menu.push(config.menu[0].clone());
        assert!(integration(config).is_err());
    }
    #[test]
    fn notification_queue_is_bounded_before_the_event_loop_starts() {
        let (client, extension) = integration(options()).unwrap();
        let handle = client.handle();
        assert!(handle.notify(1, notification()).is_ok());
        assert!(matches!(
            handle.notify(2, notification()),
            Err(DesktopError::Busy)
        ));
        assert!(matches!(
            extension.commands.try_recv().unwrap(),
            Command::Notify(1, _)
        ));
        drop(extension);
        assert!(matches!(
            handle.notify(3, notification()),
            Err(DesktopError::Unavailable)
        ));
    }
    #[test]
    fn disabled_notifications_are_not_queued_and_policy_updates_are_immediate() {
        let (client, extension) = integration(options()).unwrap();
        let handle = client.handle();
        handle.set_notifications_enabled(false);
        assert!(matches!(
            handle.notify(1, notification()),
            Err(DesktopError::Unavailable)
        ));
        assert!(extension.commands.try_recv().is_err());
        handle.set_close_to_tray(false);
        assert!(!extension.client.shared.close.load(Ordering::Relaxed));
    }
    #[test]
    fn notification_identity_is_preserved_without_domain_payloads() {
        let (client, extension) = integration(options()).unwrap();
        extension.emit(DesktopEvent::NotificationActivated(42));
        assert_eq!(
            client.try_recv(),
            Some(DesktopEvent::NotificationActivated(42))
        );
    }
}
