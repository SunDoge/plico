//! Native continuous corners, adapted from MyGO internal/darwin/window.go
//! roundContent at ee3d4bd9bd741f197de231b9fd9b002e9db55577 (MIT).
//! See ../third_party/README.md and mygo-LICENSE.
use objc2_app_kit::{NSColor, NSView};
use objc2_quartz_core::{CATransaction, kCACornerCurveContinuous};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

#[derive(Default)]
pub(crate) struct NativeWindowFrame {
    applied: Option<AppliedFrame>,
}
#[derive(Clone, Copy)]
struct AppliedFrame {
    radius: u32,
    size: [u32; 2],
    // Identity only: these addresses are never dereferenced or retained here.
    window: usize,
    layer: usize,
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
        if !content.wantsLayer() {
            content.setWantsLayer(true);
        }
        let Some(layer) = content.layer() else {
            return false;
        };
        let current = AppliedFrame {
            radius,
            size,
            window: std::ptr::from_ref(&*native) as usize,
            layer: std::ptr::from_ref(&*layer) as usize,
        };
        // MyGO configures the native mask as window style, not per-size paint.
        // Reapply after content-layer/window replacement even at identical size.
        let style_changed = self.applied.is_none_or(|old| {
            old.radius != radius || old.window != current.window || old.layer != current.layer
        }) || layer.cornerRadius() != f64::from(radius)
            || !layer.masksToBounds();
        let size_changed = self.applied.is_none_or(|old| old.size != size);
        if !style_changed && !size_changed {
            return true;
        }
        if style_changed {
            // The application background stays in its scene, inside the mask;
            // the NSWindow itself must remain clear.
            native.setOpaque(false);
            native.setBackgroundColor(Some(&NSColor::clearColor()));
            native.setHasShadow(true);
            CATransaction::begin();
            CATransaction::setDisableActions(true);
            layer.setCornerRadius(f64::from(radius));
            // SAFETY: Public constant supported since macOS 10.15, below
            // winit's deployment minimum.
            layer.setCornerCurve(unsafe { kCACornerCurveContinuous });
            layer.setMasksToBounds(true);
            CATransaction::commit();
        }
        // Geometry changes still need the system shadow to follow the mask.
        native.invalidateShadow();
        self.applied = Some(current);
        true
    }
}
