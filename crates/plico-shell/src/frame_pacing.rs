//! Shared display-link handshake; no UI declarations or native resources.
use std::cell::Cell;

#[derive(Default)]
pub(crate) struct FramePacer {
    requested: Cell<bool>,
    ready: Cell<bool>,
    running: Cell<bool>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Tick {
    Ready,
    Pause,
}
impl FramePacer {
    /// Returns (a tick is ready, the native link needs starting).
    pub fn request(&self) -> (bool, bool) {
        self.requested.set(true);
        (self.ready.replace(false), !self.running.replace(true))
    }
    pub fn tick(&self) -> Tick {
        if !self.requested.replace(false) {
            self.running.set(false);
            Tick::Pause
        } else {
            self.ready.set(true);
            Tick::Ready
        }
    }
    /// Stop immediately when idle/hidden. A late tick cannot wake the host.
    pub fn stop(&self) -> bool {
        self.requested.set(false);
        self.ready.set(false);
        self.running.replace(false)
    }
    /// An immediate input/geometry frame can consume an outstanding tick too.
    pub fn consume(&self) {
        self.ready.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn continuous_frames_follow_ticks_without_restarting_the_native_link() {
        let pacer = FramePacer::default();
        assert_eq!(pacer.request(), (false, true));
        assert_eq!(pacer.request(), (false, false));
        assert_eq!(pacer.tick(), Tick::Ready);
        assert_eq!(pacer.request(), (true, false));
        assert_eq!(pacer.tick(), Tick::Ready);
        assert_eq!(pacer.request(), (true, false));
        assert!(pacer.stop());
        assert_eq!(pacer.tick(), Tick::Pause);
        assert!(!pacer.stop());
        assert_eq!(pacer.request(), (false, true));
    }
    #[test]
    fn idle_tick_pauses_and_an_immediate_frame_consumes_queued_readiness() {
        let pacer = FramePacer::default();
        pacer.request();
        assert_eq!(pacer.tick(), Tick::Ready);
        pacer.consume();
        assert_eq!(pacer.request(), (false, false));
        assert_eq!(pacer.tick(), Tick::Ready);
        assert_eq!(pacer.tick(), Tick::Pause);
        // The host can still consume the one pending tick after a stall.
        assert_eq!(pacer.request(), (true, true));
        pacer.stop();
        assert_eq!(pacer.tick(), Tick::Pause);
        assert_eq!(pacer.request(), (false, true));
    }
}
