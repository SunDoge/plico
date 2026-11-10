use super::*;

/// Optional native integration hosted by Wabou's event loop.
///
/// Implementations create platform resources in `initialize`, enqueue events
/// from native callbacks, and drain them in `poll` on the event-loop thread.
pub trait ShellExtension {
    /// Keep semantic snapshots active without a connected platform
    /// accessibility client. Native UI test drivers use this explicitly.
    fn requires_semantics(&self) -> bool {
        false
    }

    /// Create native resources and retain `wake` for callback-to-loop delivery.
    fn initialize(&mut self, wake: WakeCallback) -> Result<(), String>;
    /// Drain native callbacks and interact with windows on the event-loop thread.
    fn poll(&mut self, context: &mut ExtensionContext<'_>);

    /// Release native resources and stop background work before the event loop
    /// and frame sources are dropped. Called at most once for extensions whose
    /// initialization was attempted.
    fn shutdown(&mut self, _context: &mut ExtensionContext<'_>) {}

    /// Return true after handling a native close request (for example by
    /// hiding the window while a tray icon keeps the process alive).
    fn close_requested(
        &mut self,
        _window_key: WindowResourceKey,
        _context: &mut ExtensionContext<'_>,
    ) -> bool {
        false
    }

    /// Observe an authoritative native size or scale transition.
    fn window_metrics_changed(
        &mut self,
        _window_key: WindowResourceKey,
        _context: &mut ExtensionContext<'_>,
    ) {
    }

    /// Observe primary-pointer movement in logical window coordinates before
    /// UI dispatch, for native overlays and pointer-driven window behavior.
    fn pointer_moved(
        &mut self,
        _window_key: WindowResourceKey,
        _position: Point,
        _context: &mut ExtensionContext<'_>,
    ) {
    }

    /// Observe the primary pointer leaving the window, including native overlays.
    fn pointer_left(
        &mut self,
        _window_key: WindowResourceKey,
        _context: &mut ExtensionContext<'_>,
    ) {
    }

    /// Handle a pointer button before it is dispatched into the UI tree.
    fn pointer_button(
        &mut self,
        _window_key: WindowResourceKey,
        _button: PointerButton,
        _phase: PointerPhase,
        _position: Point,
        _context: &mut ExtensionContext<'_>,
    ) -> bool {
        false
    }

    /// Submit an application effect owned by this extension. Returning true
    /// transfers responsibility for eventually completing the request.
    fn submit_effect(
        &mut self,
        _request: &crate::EffectRequest,
        _context: &mut ExtensionContext<'_>,
    ) -> bool {
        false
    }
}
