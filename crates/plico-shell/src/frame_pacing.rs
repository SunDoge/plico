//! Shared display-link handshake; no UI declarations or native resources.
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

/// The callback's target span is the actual CADisplayLink interval, unlike
/// `duration` on variable-rate displays. Never mix its epoch with Instant.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn display_interval(timestamp: f64, target: f64, nominal: f64) -> Duration {
    [target - timestamp, nominal, 1. / 60.]
        .into_iter()
        .filter_map(|seconds| Duration::try_from_secs_f64(seconds).ok())
        .find(|duration| !duration.is_zero())
        .expect("the fixed fallback is positive and finite")
}

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

#[cfg(test)]
mod display_interval_tests {
    use super::*;
    #[test]
    fn actual_callback_span_overrides_nominal_refresh_and_tracks_rate_changes() {
        for fps in [120., 30., 60., 144.] {
            let interval = display_interval(1000., 1000. + 1. / fps, 1. / 60.);
            assert!(interval.abs_diff(Duration::from_secs_f64(1. / fps)) < Duration::from_nanos(2));
        }
    }
    #[test]
    fn invalid_callback_spans_fall_back_without_zero_nan_or_overflow() {
        for (timestamp, target) in [
            (0., 0.),
            (2., 1.),
            (f64::NAN, 1.),
            (1., f64::INFINITY),
            (-f64::MAX, f64::MAX),
            (0., 1e-20),
        ] {
            assert_eq!(
                display_interval(timestamp, target, 1. / 120.),
                Duration::from_secs_f64(1. / 120.)
            );
        }
        for nominal in [0., -1., f64::NAN, f64::INFINITY, f64::MAX, 1e-20] {
            assert_eq!(
                display_interval(0., 0., nominal),
                Duration::from_secs_f64(1. / 60.)
            );
        }
    }
    #[test]
    fn resume_threshold_uses_callback_rate_in_both_directions() {
        let now = Instant::now();
        for (fps, pause_ms, ready) in [(120., 25, true), (30., 50, false)] {
            let pacer = FramePacer::default();
            pacer.consume(now);
            let interval = display_interval(100., 100. + 1. / fps, 1. / 60.);
            assert_eq!(
                pacer.request(now + Duration::from_millis(pause_ms), interval),
                (ready, true)
            );
        }
    }
}
