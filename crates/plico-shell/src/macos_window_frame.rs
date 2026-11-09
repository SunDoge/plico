//! Native continuous corners, adapted from MyGO internal/darwin/window.go
//! roundContent at ee3d4bd9bd741f197de231b9fd9b002e9db55577 (MIT).
//! See ../third_party/README.md and mygo-LICENSE.
use objc2_app_kit::{NSColor, NSView};
use objc2_quartz_core::{CATransaction, kCACornerCurveContinuous};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

#[derive(Default)]
pub(crate) struct NativeWindowFrame {
    applied: Option<(u32, [u32; 2])>,
}
impl NativeWindowFrame {
    /// Radius is in AppKit points, never framebuffer pixels. The native content
    /// layer masks the Metal sublayer too; do not clip the GPU scene a second time.
    pub fn apply(&mut self, window: &dyn Window, radius: u32, size: [u32; 2]) -> bool {
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return false;
        };
        // SAFETY: Shell owns the live winit view. This method is called only on
        // the AppKit event-loop thread; the borrowed view cannot escape it.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let Some(native) = view.window() else {
            return false;
        };
        let Some(content) = native.contentView() else {
            return false;
        };
        content.setWantsLayer(true);
        let Some(layer) = content.layer() else {
            return false;
        };
        // Also notice a replacement content layer after surface recreation.
        let changed = self.applied != Some((radius, size))
            || layer.cornerRadius() != f64::from(radius)
            || !layer.masksToBounds();
        if !changed {
            return true;
        }
        // Match MyGO's window setup. The application background stays in its
        // scene, inside this layer's mask; the NSWindow itself must remain clear.
        native.setOpaque(false);
        native.setBackgroundColor(Some(&NSColor::clearColor()));
        native.setHasShadow(true);
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        layer.setCornerRadius(f64::from(radius));
        // SAFETY: This public Core Animation constant is available on supported
        // macOS versions (10.15+, below winit's deployment minimum).
        layer.setCornerCurve(unsafe { kCACornerCurveContinuous });
        layer.setMasksToBounds(true);
        CATransaction::commit();
        native.invalidateShadow();
        self.applied = Some((radius, size));
        true
    }
}
