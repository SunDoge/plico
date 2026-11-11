//! User-facing view adapters and the advanced host contract.
use crate::HostAction;
use plico::Theme;

/// Adapt an ordinary application view to the native host contract.
///
/// Most applications use this implicitly through their backend's `run_view`
/// entry point. Implement [`HostView`] directly only when the application owns
/// host capabilities such as services or native wakeups.
pub struct NativeView<V>(pub V);

impl<V: plico::View> HostView for NativeView<V> {
    fn show(&mut self, ui: &mut plico::UiContext<'_>) {
        plico::View::show(&mut self.0, ui);
    }

    fn theme(&self) -> Theme {
        plico::View::theme(&self.0)
    }
}

/// Advanced host contract for native windows and retained frame sources.
///
/// Ordinary applications implement `plico::View`; `plico-app-host` adapts
/// `plico_dev::App` without coupling this runtime to the development protocol.
/// `HostView` adds host actions and native wakeups.
/// Consuming an event automatically requests a settling declaration.
pub trait HostView {
    /// A meaningful native input or accepted accessibility action occurred.
    /// Presentation frames and pointer hover do not invoke this callback.
    fn on_user_activity(&mut self) {}

    /// Window-local fallback after controls decline a key. Not called during IME
    /// composition or while a popup/modal owns input. Return true to consume it.
    fn on_unhandled_key(&mut self, _key: &plico_renderer::KeyEvent) -> bool {
        false
    }
    /// Drain one OS action. Actions remain native and never enter view snapshots.
    fn take_host_action(&mut self) -> Option<HostAction> {
        None
    }
    fn set_frame_time(&mut self, _time: plico::FrameTime) {}
    fn set_wake_callback(&mut self, _wake: plico_renderer::WakeCallback) {}
    fn poll_updates(&mut self) -> bool {
        false
    }
    /// Whether the view's own data source changed since the last rendered
    /// snapshot. Service-backed views override this so revision bumps request
    /// a declaration.
    fn snapshot_changed(&self) -> bool {
        false
    }
    fn show(&mut self, ui: &mut plico::UiContext<'_>);
    fn theme(&self) -> Theme {
        Theme::default()
    }
}
/// Local adapter: a `HostView` driven by the engine's settle loop.
pub(super) struct HostViewChild<'a, V: HostView>(pub(super) &'a mut V);
impl<V: HostView> plico::View for HostViewChild<'_, V> {
    fn theme(&self) -> plico::Theme {
        HostView::theme(self.0)
    }
    fn show(&mut self, ui: &mut plico::UiContext<'_>) {
        HostView::show(self.0, ui)
    }
}
