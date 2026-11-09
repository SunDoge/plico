//! MyGO armPass: observe only passes containing an actual presentation.
//! See ../SOURCES.md. This observes scheduling; it never suppresses application work.
use crate::frame_pass::FramePass;
use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopActivity, CFRunLoopMode, CFRunLoopObserver,
    kCFRunLoopCommonModes,
};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
struct State {
    pass: FramePass,
    callbacks: Cell<u64>,
}

pub(crate) struct NativeFramePass {
    state: Rc<State>,
    run_loop: CFRetained<CFRunLoop>,
    mode: &'static CFRunLoopMode,
    observer: Option<CFRetained<CFRunLoopObserver>>,
    available: bool,
}
impl NativeFramePass {
    pub fn new() -> Option<Self> {
        let _main_thread = MainThreadMarker::new()?;
        Some(Self {
            state: Rc::default(),
            run_loop: CFRunLoop::main()?,
            // SAFETY: The static is exported by CoreFoundation; absence is an
            // unavailable observer, never a null mode passed to CFRunLoop.
            mode: unsafe { kCFRunLoopCommonModes }?,
            observer: None,
            available: true,
        })
    }
    pub fn presented(&mut self) {
        if !self.available || !self.state.pass.presented() {
            return;
        }
        // A previous one-shot has already fired. Release its retained block
        // before replacing it, so observer/closure owners cannot accumulate.
        self.observer = None;
        let state = self.state.clone();
        let handler = RcBlock::new(move |_: *mut CFRunLoopObserver, _: CFRunLoopActivity| {
            state.callbacks.set(state.callbacks.get().saturating_add(1));
            state.pass.committed();
        });
        // SAFETY: None is the default allocator; a non-null copied block owns
        // its Rc state. Non-repeating callbacks and all mutation stay on the
        // main run loop. MyGO observes order 2_000_001, after CA's commit order;
        // this is an implementation reference, not a guaranteed Apple ordering.
        let observer = unsafe {
            CFRunLoopObserver::with_handler(
                None,
                (CFRunLoopActivity::BeforeWaiting | CFRunLoopActivity::Exit).bits(),
                false,
                2_000_001,
                Some(&handler),
            )
        };
        let Some(observer) = observer else {
            self.available = false;
            tracing::warn!("presentation-pass observation unavailable");
            return;
        };
        self.run_loop.add_observer(Some(&observer), Some(self.mode));
        self.observer = Some(observer);
    }
    pub fn snapshot(&self) -> Option<[u64; 4]> {
        self.available.then(|| {
            let [frames, completed, additional] = self.state.pass.snapshot();
            [frames, completed, additional, self.state.callbacks.get()]
        })
    }
}
impl Drop for NativeFramePass {
    fn drop(&mut self) {
        // A window may close before its pass finishes. Invalidate before its
        // state is destroyed; the main loop must retain no idle observer.
        if let Some(observer) = self.observer.take() {
            observer.invalidate();
        }
    }
}
