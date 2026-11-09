//! NSWorkspace preferences and change notifications, following MyGO
//! internal/darwin/{system,app}.go at ee3d4bd9bd741f197de231b9fd9b002e9db55577.
//! MIT attribution: ../third_party/README.md and mygo-LICENSE.
use block2::RcBlock;
use objc2::{
    rc::{Retained, autoreleasepool},
    runtime::ProtocolObject,
};
use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use objc2_foundation::{NSNotificationCenter, NSObjectProtocol};
use plico_renderer::{DisplayPreferences, WakeCallback};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Observer {
    center: Retained<NSNotificationCenter>,
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}
impl Drop for Observer {
    fn drop(&mut self) {
        // SAFETY: This token belongs to this center and is kept alive until
        // deregistration. The block captures no borrowed UI or native objects.
        let token: &ProtocolObject<dyn NSObjectProtocol> = &self.token;
        unsafe {
            self.center.removeObserver(token.as_ref());
        }
    }
}
pub(crate) struct NativePreferences {
    value: DisplayPreferences,
    dirty: Arc<AtomicBool>,
    observer: Option<Observer>,
}
impl Default for NativePreferences {
    fn default() -> Self {
        Self {
            value: Self::read(),
            dirty: Arc::new(AtomicBool::new(false)),
            observer: None,
        }
    }
}
impl NativePreferences {
    fn read() -> DisplayPreferences {
        autoreleasepool(|_| {
            let workspace = NSWorkspace::sharedWorkspace();
            DisplayPreferences {
                reduce_motion: workspace.accessibilityDisplayShouldReduceMotion(),
                reduce_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
                high_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
            }
        })
    }
    pub fn snapshot(&self) -> DisplayPreferences {
        self.value
    }
    pub fn observe(&mut self, wake: WakeCallback) {
        let dirty = self.dirty.clone();
        let notify = wake.clone();
        let block = RcBlock::new(move |_| {
            if !dirty.swap(true, Ordering::AcqRel) {
                notify();
            }
        });
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        // SAFETY: The notification has no object filter; its block is sendable
        // (only atomics and the Send+Sync event-loop wake handle). With no queue
        // it merely marks state dirty; all AppKit queries stay on the UI thread.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                None,
                None,
                &block,
            )
        };
        self.observer = Some(Observer { center, token });
        // Close the gap between the initial read and registration.
        self.dirty.store(true, Ordering::Release);
        wake();
    }
    pub fn refresh(&mut self) -> bool {
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return false;
        }
        let next = Self::read();
        let changed = next != self.value;
        self.value = next;
        changed
    }
}
