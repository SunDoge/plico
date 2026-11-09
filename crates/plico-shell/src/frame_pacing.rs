//! Shared display-link handshake; no UI declarations or native resources.
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct FramePacer {
    requested: Cell<bool>,
    ready: Cell<bool>,
    running: Cell<bool>,
    last_frame: Cell<Option<Instant>>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Tick {
    Ready,
    Pause,
}
impl FramePacer {
    /// Returns (a tick is ready, the native link needs starting).
    pub fn request(&self, now: Instant, refresh: Duration) -> (bool, bool) {
        self.requested.set(true);
        let start = !self.running.replace(true);
        let tick_ready = self.ready.replace(false);
        // Match MyGO RequestFrame: after a real pause, avoid adding a refresh
        // of input latency. A quick stop/restart must still wait for the link.
        let after_pause = start
            && self
                .last_frame
                .get()
                .is_none_or(|last| now.saturating_duration_since(last) > refresh.saturating_mul(2));
        (tick_ready || after_pause, start)
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
    pub fn consume(&self, now: Instant) {
        self.last_frame.set(Some(now));
        self.ready.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn continuous_frames_follow_ticks_without_restarting_the_native_link() {
        let pacer = FramePacer::default();
        let now = Instant::now();
        pacer.consume(now);
        assert_eq!(pacer.request(now, Duration::from_millis(16)), (false, true));
        assert_eq!(
            pacer.request(now, Duration::from_millis(16)),
            (false, false)
        );
        assert_eq!(pacer.tick(), Tick::Ready);
        assert_eq!(pacer.request(now, Duration::from_millis(16)), (true, false));
        assert_eq!(pacer.tick(), Tick::Ready);
        assert_eq!(pacer.request(now, Duration::from_millis(16)), (true, false));
        assert!(pacer.stop());
        assert_eq!(pacer.tick(), Tick::Pause);
        assert!(!pacer.stop());
        assert_eq!(pacer.request(now, Duration::from_millis(16)), (false, true));
    }
    #[test]
    fn idle_tick_pauses_and_an_immediate_frame_consumes_queued_readiness() {
        let pacer = FramePacer::default();
        let now = Instant::now();
        pacer.consume(now);
        pacer.request(now, Duration::from_millis(16));
        assert_eq!(pacer.tick(), Tick::Ready);
        pacer.consume(now);
        assert_eq!(
            pacer.request(now, Duration::from_millis(16)),
            (false, false)
        );
        assert_eq!(pacer.tick(), Tick::Ready);
        assert_eq!(pacer.tick(), Tick::Pause);
        // The host can still consume the one pending tick after a stall.
        assert_eq!(pacer.request(now, Duration::from_millis(16)), (true, true));
        pacer.stop();
        assert_eq!(pacer.tick(), Tick::Pause);
        assert_eq!(pacer.request(now, Duration::from_millis(16)), (false, true));
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;
    #[test]
    fn first_frame_and_long_pause_resume_immediately_but_quick_restart_stays_paced() {
        let now = Instant::now();
        let refresh = Duration::from_millis(10);
        let pacer = FramePacer::default();
        assert_eq!(pacer.request(now, refresh), (true, true));
        pacer.consume(now);
        assert_eq!(pacer.request(now, refresh), (false, false));
        assert!(pacer.stop());
        assert_eq!(pacer.request(now + refresh * 2, refresh), (false, true));
        pacer.stop();
        let resumed = now + refresh * 3;
        assert_eq!(pacer.request(resumed, refresh), (true, true));
        pacer.consume(resumed);
        assert_eq!(pacer.request(resumed, refresh), (false, false));
        assert_eq!(pacer.tick(), Tick::Ready);
        assert_eq!(pacer.request(resumed + refresh, refresh), (true, false));
    }
    #[test]
    fn hidden_stop_clears_tick_and_running_link_cannot_bypass_pacing_after_a_stall() {
        let now = Instant::now();
        let refresh = Duration::from_millis(8);
        let pacer = FramePacer::default();
        pacer.request(now, refresh);
        pacer.consume(now);
        assert_eq!(pacer.request(now + refresh * 10, refresh), (false, false));
        assert_eq!(pacer.tick(), Tick::Ready);
        pacer.stop();
        assert_eq!(pacer.tick(), Tick::Pause);
        assert_eq!(pacer.request(now + refresh, refresh), (false, true));
    }
}
