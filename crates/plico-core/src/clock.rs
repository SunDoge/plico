//! Host-injected monotonic logical time. UI code never reads Instant itself.
#[cfg(not(target_has_atomic = "64"))]
use portable_atomic::AtomicU64;
#[cfg(target_has_atomic = "64")]
use std::sync::atomic::AtomicU64;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use web_time::Instant;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrameTime {
    pub now: Duration,
    pub delta: Duration,
}
/// A monotonic logical time source. Pausing/scaling is a host policy.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}
impl<T: Clock + ?Sized> Clock for Arc<T> {
    fn now(&self) -> Duration {
        (**self).now()
    }
}
pub struct SystemClock {
    origin: Instant,
}
impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}
impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[derive(Debug, snafu::Snafu)]
pub enum ClockError {
    #[snafu(display("virtual clock exceeds its nanosecond range"))]
    Overflow,
    #[snafu(display("clock cannot move backwards"))]
    Backwards,
}
/// Shared deterministic clock. Advancing never sleeps or starts a thread.
#[derive(Clone, Default)]
pub struct VirtualClock {
    nanos: Arc<AtomicU64>,
}
impl Clock for VirtualClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
    }
}
impl VirtualClock {
    fn update(&self, update: impl FnMut(u64) -> Option<u64>) -> Result<u64, u64> {
        #[cfg(target_has_atomic = "64")]
        return self
            .nanos
            .try_update(Ordering::SeqCst, Ordering::SeqCst, update);
        // portable-atomic retains the older standard-library method name.
        #[cfg(not(target_has_atomic = "64"))]
        self.nanos
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, update)
    }
    pub fn advance(&self, delta: Duration) -> Result<(), ClockError> {
        let delta = u64::try_from(delta.as_nanos()).map_err(|_| ClockError::Overflow)?;
        self.update(|n| n.checked_add(delta))
            .map_err(|_| ClockError::Overflow)?;
        Ok(())
    }
    pub fn set(&self, now: Duration) -> Result<(), ClockError> {
        let now = u64::try_from(now.as_nanos()).map_err(|_| ClockError::Overflow)?;
        self.update(|previous| (now >= previous).then_some(now))
            .map_err(|_| ClockError::Backwards)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_clock_pauses_and_rejects_backwards_or_overflow() {
        let clock = VirtualClock::default();
        let external = clock.clone();
        external.advance(Duration::from_millis(10)).unwrap();
        assert_eq!(clock.now(), Duration::from_millis(10));
        assert!(clock.set(Duration::ZERO).is_err());
        assert_eq!(clock.now(), Duration::from_millis(10));
        clock.set(Duration::from_nanos(u64::MAX)).unwrap();
        assert!(clock.advance(Duration::from_nanos(1)).is_err());
    }
}
