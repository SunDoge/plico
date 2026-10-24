//! Deadline-based animation wakeups; ordinary native events can wake earlier.
use std::time::Duration;
#[derive(Default)]
pub(crate) struct FramePacer {
    deadline: Option<Duration>,
}
impl FramePacer {
    pub fn after_frame(&mut self, now: Duration, animate: bool) {
        self.deadline = animate.then(|| now.saturating_add(Duration::from_nanos(16_666_667)));
    }
    pub fn timeout(&self, now: Duration) -> Option<Duration> {
        self.deadline.map(|deadline| deadline.saturating_sub(now))
    }
    pub fn due(&self, now: Duration) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_wake_does_not_bypass_frame_cap_and_final_sample_returns_to_sleep() {
        let mut p = FramePacer::default();
        assert_eq!(p.timeout(Duration::ZERO), None);
        p.after_frame(Duration::ZERO, true);
        assert!(!p.due(Duration::from_millis(5)));
        assert!(p.timeout(Duration::from_millis(5)).unwrap() > Duration::from_millis(11));
        assert!(p.due(Duration::from_millis(17)));
        assert_eq!(p.timeout(Duration::from_millis(17)), Some(Duration::ZERO));
        p.after_frame(Duration::from_millis(17), false);
        assert_eq!(p.timeout(Duration::from_secs(1)), None);
        assert!(!p.due(Duration::from_secs(1)));
    }
}
