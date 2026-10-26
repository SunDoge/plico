//! Demo values reuse the framework's finite tweens and injected time.
use plico_motion::{Animated, Easing};
use std::time::Duration;

/// Retargets are idempotent (an unchanged target keeps the current flight) and
/// reduced motion snaps inside `Animated`, so this wrapper only owns the
/// injected instant and one scalar per animated value.
pub struct Animations {
    now: Duration,
    sidebar: Animated,
    inspector: Animated,
    progress: Animated,
}
impl Default for Animations {
    fn default() -> Self {
        Self {
            now: Duration::ZERO,
            sidebar: Animated::new(200.),
            inspector: Animated::new(0.),
            progress: Animated::new(0.),
        }
    }
}
impl Animations {
    /// Advance to the injected instant; time never runs backwards.
    pub fn advance(&mut self, now: Duration) {
        self.now = now.max(self.now);
    }
    pub fn sidebar(&self) -> f32 {
        self.sidebar.value(self.now) as f32
    }
    pub fn inspector(&self) -> f32 {
        self.inspector.value(self.now) as f32
    }
    pub fn progress(&self) -> f32 {
        self.progress.value(self.now) as f32
    }
    pub fn active(&self) -> bool {
        self.sidebar.active(self.now)
            || self.inspector.active(self.now)
            || self.progress.active(self.now)
    }
    pub fn progress_running(&self) -> bool {
        self.progress.active(self.now)
    }
    /// Apply the accessibility preference; enabling it snaps in-flight values.
    pub fn set_reduced(&mut self, reduced: bool) {
        self.sidebar.set_reduced(reduced);
        self.inspector.set_reduced(reduced);
        self.progress.set_reduced(reduced);
    }
    /// Declare the layout targets for this frame. Unchanged targets keep
    /// their current flight; reduced motion completes immediately.
    pub fn targets(&mut self, compact: bool, inspector: bool) {
        let duration = Duration::from_millis(260);
        self.sidebar.retarget(
            self.now,
            if compact { 72. } else { 200. },
            duration,
            Easing::EaseOut,
        );
        self.inspector.retarget(
            self.now,
            if inspector { 248. } else { 0. },
            duration,
            Easing::EaseOut,
        );
    }
    /// Restart the progress sweep from zero toward one.
    pub fn start(&mut self) {
        self.progress.snap(0.);
        self.progress
            .retarget(self.now, 1., Duration::from_millis(2400), Easing::EaseInOut);
    }
    /// Stop the sweep and keep the value visible right now.
    pub fn cancel(&mut self) {
        self.progress.snap(self.progress.value(self.now));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_settle_time_does_not_advance_and_retarget_is_continuous() {
        let mut a = Animations::default();
        a.advance(Duration::ZERO);
        a.targets(true, true);
        a.advance(Duration::from_millis(100));
        let width = a.sidebar();
        assert!(width > 72. && width < 200.);
        a.advance(Duration::from_millis(100));
        assert_eq!(a.sidebar(), width);
        a.targets(false, false);
        assert_eq!(a.sidebar(), width, "an unchanged target keeps the flight");
        a.advance(Duration::from_millis(400));
        assert_eq!(a.sidebar(), 200.);
        assert_eq!(a.inspector(), 0.);
        assert!(!a.active());
    }
    #[test]
    fn finite_progress_and_reduced_motion_stop_requesting_frames() {
        let mut a = Animations::default();
        a.advance(Duration::ZERO);
        a.start();
        a.advance(Duration::from_millis(1200));
        assert!((a.progress() - 0.5).abs() < 0.001);
        a.advance(Duration::from_millis(2400));
        assert_eq!(a.progress(), 1.);
        assert!(!a.active());
        a.set_reduced(true);
        a.targets(true, true);
        assert_eq!(a.sidebar(), 72.);
        assert_eq!(a.inspector(), 248.);
        assert!(!a.active());
        a.set_reduced(false);
        a.start();
        a.advance(Duration::from_millis(700));
        a.cancel();
        assert!(!a.active());
        // Cancellation freezes exactly the sampled value, without rounding
        // through f32, and later frames keep reporting it.
        let frozen = a.progress();
        a.advance(Duration::from_millis(1000));
        assert_eq!(a.progress(), frozen);
        a.advance(Duration::from_millis(5000));
        assert_eq!(a.progress(), frozen);
    }
}
