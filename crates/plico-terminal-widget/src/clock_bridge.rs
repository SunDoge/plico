//! Map Plico's injected logical duration onto the existing native deadline API.
use std::{sync::Arc, time::Instant};
pub(super) struct TerminalClock {
    clock: Arc<dyn plico_core::Clock>,
    epoch: Instant,
}
impl TerminalClock {
    pub fn new(clock: Arc<dyn plico_core::Clock>) -> Self {
        Self {
            clock,
            epoch: Instant::now(),
        }
    }
    pub fn system() -> Self {
        Self::new(Arc::new(plico_core::SystemClock::default()))
    }
    pub fn now_instant(&self) -> Instant {
        self.epoch
            .checked_add(self.clock.now())
            .unwrap_or(self.epoch)
    }
}
