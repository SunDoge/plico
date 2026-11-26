use crate::UiContext;
use std::time::Duration;

/// A frame of a content replacement, with normalized linear progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentTransitionFrame<T> {
    pub from: T,
    pub to: T,
    pub progress: f64,
}

/// A timed transition between discrete content values, sampled by the
/// declaration clock. The owner chooses how to draw and ease each frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentTransition<T> {
    current: Option<(T, T)>,
    started: Option<Duration>,
}

impl<T> Default for ContentTransition<T> {
    fn default() -> Self {
        Self {
            current: None,
            started: None,
        }
    }
}

impl<T: Copy + Eq> ContentTransition<T> {
    /// Sample and wake the host only while the replacement is active. The
    /// first value and reduced-motion changes are presented immediately.
    /// A new value during flight uses the previous target as its source.
    pub fn sample(
        &mut self,
        cx: &mut UiContext,
        target: T,
        duration: Duration,
    ) -> ContentTransitionFrame<T> {
        let frame = self.advance(
            cx.now(),
            cx.display_preferences().reduce_motion,
            target,
            duration,
        );
        if frame.progress < 1. {
            cx.request_animation_frame();
        }
        frame
    }

    fn advance(
        &mut self,
        now: Duration,
        reduce_motion: bool,
        target: T,
        duration: Duration,
    ) -> ContentTransitionFrame<T> {
        let mut content = self.current.unwrap_or((target, target));
        if content.1 != target {
            content = (content.1, target);
            self.started = Some(now);
        }
        let elapsed = self.started.map(|start| now.saturating_sub(start));
        let progress = match elapsed {
            Some(elapsed) if !reduce_motion && elapsed < duration => {
                elapsed.as_secs_f64() / duration.as_secs_f64()
            }
            _ => {
                content = (target, target);
                self.started = None;
                1.
            }
        };
        self.current = Some(content);
        ContentTransitionFrame {
            from: content.0,
            to: content.1,
            progress,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_change_retarget_and_reduced_motion() {
        let mut motion = ContentTransition::default();
        let duration = Duration::from_millis(200);
        let mut frame = |ms, target, reduced| {
            motion.advance(Duration::from_millis(ms), reduced, target, duration)
        };
        assert_eq!(frame(0, 6, false).progress, 1.);
        assert_eq!(
            frame(20, 7, false),
            ContentTransitionFrame {
                from: 6,
                to: 7,
                progress: 0.
            }
        );
        assert_eq!(frame(120, 7, false).progress, 0.5);
        assert_eq!(
            frame(120, 8, false),
            ContentTransitionFrame {
                from: 7,
                to: 8,
                progress: 0.
            }
        );
        assert_eq!(
            frame(130, 8, true),
            ContentTransitionFrame {
                from: 8,
                to: 8,
                progress: 1.
            }
        );
        assert_eq!(frame(150, 8, false).progress, 1.);
    }
}
