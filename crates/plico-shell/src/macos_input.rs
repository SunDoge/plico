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

/// Owned native target lets the host prepare renderer state after parsing the
/// event, before AppKit changes visibility. It cannot leave the AppKit thread.
pub(crate) struct PreparedTitlebarAction {
    window: objc2::rc::Retained<objc2_app_kit::NSWindow>,
    action: crate::titlebar_action::TitlebarAction,
}
impl PreparedTitlebarAction {
    pub fn minimizes(&self) -> bool {
        self.action == crate::titlebar_action::TitlebarAction::Minimize
    }
    pub fn perform(self) {
        use crate::titlebar_action::TitlebarAction;
        match self.action {
            TitlebarAction::Ignore => {}
            TitlebarAction::Minimize => self.window.performMiniaturize(None),
            TitlebarAction::Zoom => self.window.performZoom(None),
        }
    }
}

#[derive(Default)]
pub(crate) struct NativeInput {
    titlebar_event: crate::titlebar_action::TitlebarEvent,
}
impl NativeInput {
    /// Parse/consume in winit's current drag gesture, retaining the native target
    /// so renderer preparation can precede a visibility-changing action.
    pub(crate) fn prepare_titlebar_double_click(
        &self,
        window: &dyn winit::window::Window,
    ) -> Option<PreparedTitlebarAction> {
        use crate::titlebar_action::{TitlebarAction, titlebar_action};
        use objc2::{MainThreadMarker, rc::autoreleasepool};
        use objc2_app_kit::{NSApplication, NSEventType, NSView};
        use objc2_foundation::{NSString, NSUserDefaults};
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let mtm = MainThreadMarker::new()?;
        autoreleasepool(|_| {
            let handle = window.window_handle().ok()?;
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return None;
            };
            // SAFETY: A live winit-owned view, borrowed only on the AppKit thread.
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            let native = view.window()?;
            let event = NSApplication::sharedApplication(mtm).currentEvent()?;
            let left_down = event.r#type() == NSEventType::LeftMouseDown;
            let same_window = event.windowNumber() == native.windowNumber();
            // clickCount is only defined for mouse events. Check the event kind
            // before asking AppKit for it (keyboard/menu commands can reach here).
            if !left_down || !same_window {
                return None;
            }
            let count = event.clickCount();
            titlebar_action(left_down, same_window, count, None)?;
            if !self
                .titlebar_event
                .consume_once(event.eventNumber(), event.timestamp())
            {
                return Some(PreparedTitlebarAction {
                    window: native,
                    action: TitlebarAction::Ignore,
                });
            }
            // MyGO internal/darwin/window.go::TitleBarDoubleClicked reads this exact
            // AppKit preference. Do not use an application-persisted replacement.
            let preference = NSUserDefaults::standardUserDefaults()
                .stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))
                .map(|value| value.to_string());
            let action = titlebar_action(left_down, same_window, count, preference.as_deref())?;
            Some(PreparedTitlebarAction {
                window: native,
                action,
            })
        })
    }
}
