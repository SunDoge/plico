//! One-shot presentation-pass accounting, following MyGO's armPass/caPass.
//! The native observer owns timing; this type starts no thread or polling loop.
use std::cell::Cell;

#[derive(Default)]
pub(crate) struct FramePass {
    frames: Cell<u64>,
    completed: Cell<u64>,
    additional: Cell<u64>,
    in_pass: Cell<u64>,
}
impl FramePass {
    /// Record a real presentation. Only the first in a pass needs an observer.
    pub fn presented(&self) -> bool {
        self.frames.set(self.frames.get().saturating_add(1));
        let previous = self.in_pass.get();
        self.in_pass.set(previous.saturating_add(1));
        if previous != 0 {
            self.additional.set(self.additional.get().saturating_add(1));
        }
        previous == 0
    }
    /// BeforeWaiting/Exit may both occur, but an armed observer counts once.
    pub fn committed(&self) {
        if self.in_pass.replace(0) != 0 {
            self.completed.set(self.completed.get().saturating_add(1));
        }
    }
    /// Presentations, completed active passes, additional presentations in a pass.
    pub fn snapshot(&self) -> [u64; 3] {
        [
            self.frames.get(),
            self.completed.get(),
            self.additional.get(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_presentations_arm_one_observer_and_count_additional_frames() {
        let pass = FramePass::default();
        assert!(pass.presented());
        assert!(!pass.presented());
        assert!(!pass.presented());
        assert_eq!(pass.snapshot(), [3, 0, 2]);
        pass.committed();
        assert_eq!(pass.snapshot(), [3, 1, 2]);
        assert!(pass.presented());
        pass.committed();
        assert_eq!(pass.snapshot(), [4, 2, 2]);
    }
    #[test]
    fn idle_and_duplicate_callbacks_cannot_manufacture_active_passes() {
        let pass = FramePass::default();
        for _ in 0..100 {
            pass.committed();
        }
        assert_eq!(pass.snapshot(), [0; 3]);
        assert!(pass.presented());
        pass.committed();
        pass.committed();
        assert_eq!(pass.snapshot(), [1, 1, 0]);
    }
    #[test]
    fn independent_windows_do_not_share_presentation_counts() {
        let a = FramePass::default();
        let b = FramePass::default();
        assert!(a.presented());
        assert!(b.presented());
        assert!(!a.presented());
        a.committed();
        assert_eq!(a.snapshot(), [2, 1, 1]);
        assert_eq!(b.snapshot(), [1, 0, 0]);
        b.committed();
        assert_eq!(b.snapshot(), [1, 1, 0]);
    }
}
