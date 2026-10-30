//! User-facing view adapters and the advanced host contract.
use crate::{HostAction, regions};
use plico::{Engine, Entity, Theme};

/// Adapt an ordinary application view to the native host contract.
///
/// Most applications use this implicitly through their backend's `run_view`
/// entry point. Implement [`HostView`] directly only when the application owns
/// host capabilities such as services, hot reload, Regions, or native wakeups.
pub struct NativeView<V>(pub V);

impl<V: plico::View> HostView for NativeView<V> {
    fn show(&mut self, ui: &mut plico::UiContext<'_>) {
        plico::View::show(&mut self.0, ui);
    }

    fn theme(&self) -> Theme {
        plico::View::theme(&self.0)
    }
}

/// A statically linked application using the same App implementation as hot reload.
pub struct NativeApp<A>(pub A);
/// Shared forwarding for both the wrapper and application's direct HostView implementation.
#[doc(hidden)]
#[macro_export]
macro_rules! __plico_app_host_view_methods {
    ($this:ident, $app:expr, $shared:expr) => {
        fn set_frame_time(&mut $this, time: $crate::__ui_core::FrameTime) {
            $crate::__app_protocol::App::set_frame_time($app, time);
        }
        fn on_unhandled_key(&mut $this, key: &$crate::KeyEvent) -> bool {
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
/// Implement HostView using the same forwarding as NativeApp.
#[macro_export]
macro_rules! impl_host_view_for_app {
    ($app:ty) => {
        impl $crate::HostView for $app {
            $crate::__plico_app_host_view_methods!(self, self, self);
        }
    };
}
impl<A: plico_dev::App> HostView for NativeApp<A> {
    crate::__plico_app_host_view_methods!(self, &mut self.0, &self.0);
}
/// Advanced host contract for native windows and retained frame sources.
///
/// Ordinary applications implement `plico::View` or `plico_dev::App` instead.
/// `HostView` adds host actions, wakeups, reload, Regions, and cache metadata.
/// Consuming an event automatically requests a settling declaration.
pub trait HostView {
    #[doc(hidden)]
    fn replay_regions(
        &mut self,
        _engine: &mut Engine,
    ) -> Option<Result<regions::ReplayBatch, plico::FrameError>> {
        None
    }
    #[doc(hidden)]
    fn scene_cache_roots(&self) -> Vec<Entity> {
        Vec::new()
    }
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
    fn poll_reload(&mut self) -> bool {
        false
    }
    /// Whether the view's own data source changed since the last rendered
    /// snapshot. Service-backed views override this so revision bumps request
    /// a declaration; reactive views may replay only their dirty regions.
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
