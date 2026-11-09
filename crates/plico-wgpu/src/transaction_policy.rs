//! MyGO's resize-after-transaction rule, independent of AppKit and GPU handles.
use crate::PresentationTick;
use std::time::{Duration, Instant};
#[derive(Clone, Copy)]
pub(crate) struct PassBoundary {
    pub epoch: u64,
    pub committed_at: Option<Instant>,
}
struct Wait {
    tick: Option<u64>,
    began: Instant,
    deadline: Instant,
    released: bool,
}
#[derive(Default)]
pub(crate) struct TransactionPolicy {
    last_extent: Option<[u32; 2]>,
    last_sync: bool,
    last_sync_epoch: u64,
    wait: Option<Wait>,
    stats: [u64; 3],
}
impl TransactionPolicy {
    pub fn requires_boundary(&self, extent: [u32; 2], pass_epoch: u64) -> bool {
        self.last_sync
            && self
                .last_extent
                .is_some_and(|old| old != extent || pass_epoch == self.last_sync_epoch)
    }
    pub fn defer(
        &mut self,
        extent: [u32; 2],
        live_resize: bool,
        tick: Option<PresentationTick>,
        now: Instant,
        interval: Duration,
        pass: PassBoundary,
    ) -> bool {
        if live_resize || !self.requires_boundary(extent, pass.epoch) {
            self.wait = None;
            return false;
        }
        let interval = if interval.is_zero() {
            Duration::from_secs_f64(1. / 60.)
        } else {
            interval
        };
        let later = |time: Instant| {
            time.checked_add(interval)
                .or_else(|| time.checked_add(Duration::from_secs_f64(1. / 60.)))
                .unwrap_or(time)
        };
        let wait = self.wait.get_or_insert_with(|| {
            self.stats[2] += 1;
            Wait {
                tick: tick.map(|t| t.serial),
                began: now,
                deadline: later(now),
                released: false,
            }
        });
        if pass.epoch == self.last_sync_epoch {
            // Yield for the CA observer; an overdue fallback must not spin.
            if tick.is_none() && now >= wait.deadline {
                wait.deadline = later(now);
            }
            return true;
        }
        let boundary = pass.committed_at.unwrap_or(now).max(wait.began);
        wait.deadline = later(boundary);
        wait.released |= match (wait.tick, tick) {
            (Some(before), Some(after)) => {
                after.serial != before && after.observed_at.is_some_and(|time| time > boundary)
            }
            (None, Some(after)) => after.observed_at.is_some_and(|time| time > boundary),
            (_, None) => now >= wait.deadline,
        };
        !wait.released
    }
    pub fn deferred(&self) -> bool {
        self.wait.as_ref().is_some_and(|wait| !wait.released)
    }
    /// Fallback hint; a host with display ticks ignores this deadline.
    pub fn deadline(&self) -> Option<Instant> {
        self.wait
            .as_ref()
            .filter(|w| !w.released)
            .map(|w| w.deadline)
    }
    pub fn cancel(&mut self) {
        self.wait = None;
    }
    pub fn presented(&mut self, extent: [u32; 2], synced: bool, pass_epoch: u64) {
        self.last_extent = Some(extent);
        self.last_sync = synced;
        self.last_sync_epoch = pass_epoch;
        self.wait = None;
        self.stats[usize::from(!synced)] += 1;
    }
    /// Synchronized submissions, immediate submissions, newly deferred updates.
    pub fn stats(&self) -> [u64; 3] {
        self.stats
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tick(serial: u64, time: Instant) -> Option<PresentationTick> {
        Some(PresentationTick {
            serial,
            observed_at: Some(time),
        })
    }
    fn pass(epoch: u64, time: Instant) -> PassBoundary {
        PassBoundary {
            epoch,
            committed_at: Some(time),
        }
    }
    #[test]
    fn resize_after_sync_coalesces_until_a_fresh_tick_then_preserves_retry_permission() {
        let now = Instant::now();
        let interval = Duration::from_millis(8);
        let previous = now - interval;
        let mut policy = TransactionPolicy::default();
        policy.presented([100, 100], true, 0);
        assert!(policy.defer(
            [200, 100],
            false,
            tick(1, previous),
            now,
            interval,
            pass(1, previous)
        ));
        assert!(policy.defer(
            [250, 100],
            false,
            tick(1, previous),
            now + interval * 3,
            interval,
            pass(1, previous)
        ));
        assert_eq!(policy.stats(), [1, 0, 1]);
        assert!(!policy.defer(
            [250, 100],
            false,
            tick(2, now + interval),
            now + interval,
            interval,
            pass(1, previous)
        ));
        assert!(!policy.defer(
            [270, 100],
            false,
            tick(2, now + interval),
            now + interval,
            interval,
            pass(1, previous)
        ));
        policy.presented([270, 100], true, 1);
        assert!(!policy.deferred());
    }
    #[test]
    fn live_resize_input_reversion_and_first_frame_are_not_delayed() {
        let now = Instant::now();
        let interval = Duration::from_millis(16);
        let boundary = pass(1, now);
        let mut policy = TransactionPolicy::default();
        assert!(!policy.defer([100, 100], false, None, now, interval, boundary));
        policy.presented([100, 100], true, 0);
        assert!(!policy.defer([200, 100], true, None, now, interval, boundary));
        assert!(!policy.defer([100, 100], false, None, now, interval, boundary));
        assert!(policy.defer([200, 100], false, None, now, interval, boundary));
        assert!(!policy.defer([100, 100], false, None, now, interval, boundary));
        policy.presented([100, 100], false, 0);
        assert!(!policy.defer([200, 100], false, None, now, interval, boundary));
    }
    #[test]
    fn fallback_deadline_is_stable_and_lifecycle_cancel_clears_it() {
        let now = Instant::now();
        let interval = Duration::from_millis(16);
        let previous = now - interval;
        let mut policy = TransactionPolicy::default();
        policy.presented([100, 100], true, 0);
        assert!(policy.defer([200, 100], false, None, now, interval, pass(1, previous)));
        assert_eq!(policy.deadline(), Some(now + interval));
        assert!(policy.defer(
            [250, 100],
            false,
            None,
            now + interval / 2,
            interval,
            pass(1, previous)
        ));
        assert_eq!(policy.deadline(), Some(now + interval));
        assert!(!policy.defer(
            [250, 100],
            false,
            None,
            now + interval,
            interval,
            pass(1, previous)
        ));
        policy.cancel();
        assert_eq!(policy.deadline(), None);
    }
    #[test]
    fn late_tick_before_commit_does_not_release_but_first_tick_after_commit_does() {
        let now = Instant::now();
        let interval = Duration::from_millis(8);
        let mut policy = TransactionPolicy::default();
        policy.presented([100, 100], true, 7);
        let before = PassBoundary {
            epoch: 7,
            committed_at: None,
        };
        assert!(policy.defer(
            [200, 100],
            false,
            tick(10, now - interval),
            now,
            interval,
            before
        ));
        assert!(policy.defer(
            [200, 100],
            false,
            tick(11, now + interval),
            now + interval,
            interval,
            before
        ));
        let committed = pass(8, now + interval * 2);
        assert!(policy.defer(
            [200, 100],
            false,
            tick(11, now + interval),
            now + interval * 2,
            interval,
            committed
        ));
        assert!(!policy.defer(
            [200, 100],
            false,
            tick(12, now + interval * 3),
            now + interval * 3,
            interval,
            committed
        ));
    }
    #[test]
    fn overdue_fallback_yields_for_commit_without_adding_a_whole_extra_period() {
        let now = Instant::now();
        let interval = Duration::from_millis(16);
        let mut policy = TransactionPolicy::default();
        policy.presented([100, 100], true, 0);
        let before = PassBoundary {
            epoch: 0,
            committed_at: None,
        };
        assert!(policy.defer([200, 100], false, None, now, interval, before));
        assert!(policy.defer([200, 100], false, None, now + interval, interval, before));
        assert_eq!(policy.deadline(), Some(now + interval * 2));
        let committed = pass(1, now + interval + Duration::from_millis(1));
        assert!(policy.defer(
            [200, 100],
            false,
            None,
            now + interval * 2,
            interval,
            committed
        ));
        assert_eq!(
            policy.deadline(),
            Some(now + interval * 2 + Duration::from_millis(1))
        );
        assert!(!policy.defer(
            [200, 100],
            false,
            None,
            now + interval * 2 + Duration::from_millis(1),
            interval,
            committed
        ));
    }
    #[test]
    fn changed_content_after_sync_in_the_same_pass_waits_without_being_dropped() {
        let now = Instant::now();
        let interval = Duration::from_millis(8);
        let mut policy = TransactionPolicy::default();
        policy.presented([100, 100], true, 3);
        let old = PassBoundary {
            epoch: 3,
            committed_at: None,
        };
        assert!(policy.defer(
            [100, 100],
            false,
            tick(1, now - interval),
            now,
            interval,
            old
        ));
        assert!(!policy.defer(
            [100, 100],
            false,
            tick(2, now + interval),
            now + interval,
            interval,
            pass(4, now)
        ));
        policy.presented([100, 100], false, 4);
        assert_eq!(policy.stats(), [1, 1, 1]);
    }
}
