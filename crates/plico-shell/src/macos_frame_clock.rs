//! NSView-associated animation cadence, following MyGO darwin/surface.go.
//! See ../third_party/README.md for MIT source attribution.
use crate::frame_pacing::{FramePacer, Tick};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_app_kit::NSView;
use objc2_foundation::{NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::{CADisplayLink, CAFrameRateRange};
use plico_renderer::WakeCallback;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::rc::Rc;
use winit::window::Window;

struct State {
    pacer: FramePacer,
    wake: WakeCallback,
}
define_class!(
    // SAFETY: NSObject has no subclassing requirements. All calls occur on
    // the main run loop and ivars contain no borrowed native/UI resources.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Rc<State>]
    #[name = "PlicoDisplayLinkTarget"]
    struct DisplayLinkTarget;
    unsafe impl NSObjectProtocol for DisplayLinkTarget {}
    impl DisplayLinkTarget {
        #[unsafe(method(plicoDisplayTick:))]
        fn tick(&self, link: &CADisplayLink) {
            let state = self.ivars();
            match state.pacer.tick() {
                Tick::Ready => (state.wake)(),
                Tick::Pause => link.setPaused(true),
            }
        }
    }
);
impl DisplayLinkTarget {
    fn new(mtm: MainThreadMarker, state: Rc<State>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(state);
        // SAFETY: Initialize an allocated NSObject subclass with its ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

pub(crate) struct NativeFrameClock {
    link: Retained<CADisplayLink>,
    state: Rc<State>,
}
impl NativeFrameClock {
    pub fn new(window: &dyn Window, wake: WakeCallback) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: The caller owns a live winit view on the AppKit thread.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let selector = sel!(displayLinkWithTarget:selector:);
        if !view.respondsToSelector(selector) {
            return None;
        }
        let state = Rc::new(State {
            pacer: FramePacer::default(),
            wake,
        });
        let target = DisplayLinkTarget::new(mtm, state.clone());
        // SAFETY: Selector has the exact CADisplayLink callback signature above;
        // the link retains the target. NSView associates it with its display.
        let link = unsafe { view.displayLinkWithTarget_selector(&target, sel!(plicoDisplayTick:)) };
        link.setPaused(true);
        // MyGO's default range follows the current display, including ProMotion.
        link.setPreferredFrameRateRange(CAFrameRateRange {
            minimum: 0.,
            maximum: 0.,
            preferred: 0.,
        });
        // SAFETY: Creation and callbacks stay on the main thread. Common modes
        // preserve animation ticks during AppKit tracking/live resize.
        unsafe {
            link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes);
        }
        Some(Self { link, state })
    }
    /// Continuous animation waits for a tick; other work is immediately ready.
    pub fn frame_ready(&self, continuous: bool) -> bool {
        if !continuous {
            if self.state.pacer.stop() {
                self.link.setPaused(true);
            }
            return true;
        }
        let (ready, start) = self.state.pacer.request();
        if start {
            self.link.setPaused(false);
        }
        ready
    }
    pub fn consume(&self) {
        self.state.pacer.consume();
    }
}
impl Drop for NativeFrameClock {
    fn drop(&mut self) {
        self.state.pacer.stop();
        // Break the run-loop/link/target ownership before destroying the window.
        self.link.invalidate();
    }
}
