//! Native click thresholds and titlebar actions. Selection receives a threshold;
//! titlebar drag commands use the current NSEvent, like winit. See third_party.
use plico_renderer::MultiClickSettings;

pub(crate) fn multi_click_settings() -> MultiClickSettings {
    let mut settings = MultiClickSettings::default();
    if let Ok(interval) =
        std::time::Duration::try_from_secs_f64(objc2_app_kit::NSEvent::doubleClickInterval())
        && !interval.is_zero()
    {
        settings.interval = interval;
    }
    settings
}

#[derive(Default)]
pub(crate) struct NativeInput {
    titlebar_event: crate::titlebar_action::TitlebarEvent,
}
impl NativeInput {
    /// Consume a titlebar double click in the same native event context used by
    /// winit's drag_window; single clicks and other windows retain their drag path.
    pub(crate) fn handle_titlebar_double_click(&self, window: &dyn winit::window::Window) -> bool {
        use crate::titlebar_action::{TitlebarAction, titlebar_action};
        use objc2::{MainThreadMarker, rc::autoreleasepool};
        use objc2_app_kit::{NSApplication, NSEventType, NSView};
        use objc2_foundation::{NSString, NSUserDefaults};
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        autoreleasepool(|_| {
            let Ok(handle) = window.window_handle() else {
                return false;
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return false;
            };
            // SAFETY: A live winit-owned view, borrowed only on the AppKit thread.
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            let Some(native) = view.window() else {
                return false;
            };
            let Some(event) = NSApplication::sharedApplication(mtm).currentEvent() else {
                return false;
            };
            let left_down = event.r#type() == NSEventType::LeftMouseDown;
            let same_window = event.windowNumber() == native.windowNumber();
            // clickCount is only defined for mouse events. Check the event kind
            // before asking AppKit for it (keyboard/menu commands can reach here).
            if !left_down || !same_window {
                return false;
            }
            let count = event.clickCount();
            if titlebar_action(left_down, same_window, count, None).is_none() {
                return false;
            }
            if !self
                .titlebar_event
                .consume_once(event.eventNumber(), event.timestamp())
            {
                return true;
            }
            // MyGO internal/darwin/window.go::TitleBarDoubleClicked reads this exact
            // AppKit preference. Do not use an application-persisted replacement.
            let preference = NSUserDefaults::standardUserDefaults()
                .stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))
                .map(|value| value.to_string());
            match titlebar_action(left_down, same_window, count, preference.as_deref()) {
                Some(TitlebarAction::Ignore) => {}
                Some(TitlebarAction::Minimize) => native.performMiniaturize(None),
                Some(TitlebarAction::Zoom) => native.performZoom(None),
                None => return false,
            }
            true
        })
    }
}
