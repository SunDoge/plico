//! Demo values reuse the framework's finite tweens and injected time.
use plico_motion::{Easing, MotionArena, MotionId};
use std::time::Duration;

pub struct Animations {
    arena: MotionArena<1>,
    sidebar: MotionId,
    inspector: MotionId,
    progress: MotionId,
    last_time: Option<Duration>,
    sampled: Vec<MotionId>,
    sidebar_target: f64,
    inspector_target: f64,
}
impl Default for Animations {
    fn default() -> Self {
        let mut arena = MotionArena::default();
        let sidebar = arena.insert_tween([200.], [200.], Duration::ZERO, Easing::EaseOut);
        let inspector = arena.insert_tween([0.], [0.], Duration::ZERO, Easing::EaseOut);
        let progress = arena.insert_tween([0.], [0.], Duration::ZERO, Easing::Linear);
        Self {
            arena,
            sidebar,
            inspector,
            progress,
            last_time: None,
            sampled: Vec::new(),
            sidebar_target: 200.,
            inspector_target: 0.,
        }
    }
}
impl Animations {
    pub fn advance(&mut self, now: Duration) {
        let previous = self.last_time.unwrap_or(now);
        let now = now.max(previous);
        self.arena
            .advance_into(now.saturating_sub(previous), &mut self.sampled);
        self.last_time = Some(now);
    }
    pub fn sidebar(&self) -> f32 {
        self.arena.value(self.sidebar).unwrap()[0] as f32
    }
    pub fn inspector(&self) -> f32 {
        self.arena.value(self.inspector).unwrap()[0] as f32
    }
    pub fn progress(&self) -> f32 {
        self.arena.value(self.progress).unwrap()[0] as f32
    }
    pub fn active(&self) -> bool {
        self.arena.is_active()
    }
    pub fn progress_running(&self) -> bool {
        self.arena.is_running(self.progress)
    }
    pub fn targets(&mut self, compact: bool, inspector: bool, reduced: bool) {
        let duration = if reduced {
            Duration::ZERO
        } else {
            Duration::from_millis(260)
        };
        let sidebar = if compact { 72. } else { 200. };
        if self.sidebar_target != sidebar || reduced {
            self.arena
                .retarget_tween(self.sidebar, [sidebar], duration, Easing::EaseOut);
            self.sidebar_target = sidebar;
        }
        let width = if inspector { 248. } else { 0. };
        if self.inspector_target != width || reduced {
            self.arena
                .retarget_tween(self.inspector, [width], duration, Easing::EaseOut);
            self.inspector_target = width;
        }
        if reduced && self.progress_running() {
            self.arena
                .retarget_tween(self.progress, [1.], Duration::ZERO, Easing::Linear);
        }
    }
    pub fn start(&mut self, reduced: bool) {
        self.arena
            .retarget_tween(self.progress, [0.], Duration::ZERO, Easing::Linear);
        self.arena.retarget_tween(
            self.progress,
            [1.],
            if reduced {
                Duration::ZERO
            } else {
                Duration::from_millis(2400)
            },
            Easing::EaseInOut,
        );
    }
    pub fn cancel(&mut self) {
        let current = self.progress();
        self.arena.retarget_tween(
            self.progress,
            [current as f64],
            Duration::ZERO,
            Easing::Linear,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_settle_time_does_not_advance_and_retarget_is_continuous() {
        let mut a = Animations::default();
        a.advance(Duration::ZERO);
        a.targets(true, true, false);
        a.advance(Duration::from_millis(100));
        let width = a.sidebar();
        assert!(width > 72. && width < 200.);
        a.advance(Duration::from_millis(100));
        assert_eq!(a.sidebar(), width);
        a.targets(false, false, false);
        assert_eq!(a.sidebar(), width);
        a.advance(Duration::from_millis(400));
        assert_eq!(a.sidebar(), 200.);
        assert_eq!(a.inspector(), 0.);
        assert!(!a.active());
    }
    #[test]
    fn finite_progress_and_reduced_motion_stop_requesting_frames() {
        let mut a = Animations::default();
        a.advance(Duration::ZERO);
        a.start(false);
        a.advance(Duration::from_millis(1200));
        assert!((a.progress() - 0.5).abs() < 0.001);
        a.advance(Duration::from_millis(2400));
        assert_eq!(a.progress(), 1.);
        assert!(!a.active());
        a.targets(true, true, true);
        assert_eq!(a.sidebar(), 72.);
        assert_eq!(a.inspector(), 248.);
        assert!(!a.active());
        a.start(false);
        a.cancel();
        assert!(!a.active());
    }
}
