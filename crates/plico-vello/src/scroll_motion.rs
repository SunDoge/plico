//! Host-owned scroll interpolation driven exclusively by the injected frame clock.
use std::time::Duration;

pub(super) struct ScrollMotion {
    from: f32,
    pub target: f32,
    start: Duration,
}
impl ScrollMotion {
    pub fn new(from: f32, target: f32, start: Duration) -> Self {
        Self {
            from,
            target,
            start,
        }
    }
    pub fn sample(&self, now: Duration) -> (f32, bool) {
        let t = (now.saturating_sub(self.start).as_secs_f64() / 0.16).clamp(0., 1.);
        let value = self.from + (self.target - self.from) * (1. - (1. - t).powi(3)) as f32;
        (if t == 1. { self.target } else { value }, t < 1.)
    }
}
#[derive(Default)]
pub(super) struct ScrollAdvance {
    pub changed: bool,
    pub declarations: bool,
}
impl crate::Adapter {
    pub(super) fn advance_scroll(
        &mut self,
        now: Duration,
        engine: &plico_core::Engine,
    ) -> ScrollAdvance {
        let mut result = ScrollAdvance::default();
        self.scroll_motions.retain(|id, motion| {
            let (value, active) = motion.sample(now);
            let offset = self.scroll_offsets.entry(*id).or_default();
            if offset[1] != value {
                result.changed = true;
                result.declarations |= self
                    .reverse
                    .get(id)
                    .is_some_and(|entity| engine.observes_scroll(*entity));
            }
            offset[1] = value;
            active
        });
        self.scroll_dirty |= result.changed;
        result
    }
    pub(super) fn scroll_to(
        &mut self,
        id: taffy::NodeId,
        target: f32,
        smooth: bool,
        now: Duration,
    ) {
        let current = self.scroll_offsets.get(&id).map_or(0., |p| p[1]);
        if smooth && self.smooth_scroll_enabled && current != target {
            self.scroll_motions
                .insert(id, ScrollMotion::new(current, target, now));
        } else {
            self.scroll_motions.remove(&id);
            self.scroll_offsets.entry(id).or_default()[1] = target;
            self.scroll_dirty |= current != target;
        }
    }
    pub(super) fn finish_scroll(&mut self) {
        for (id, motion) in self.scroll_motions.drain() {
            self.scroll_offsets.entry(id).or_default()[1] = motion.target;
            self.scroll_dirty = true;
        }
    }
}
