// Adapted from MyGO plugins/render/frame.go at 13a0b3e7 (MIT).
use plico::{Easing, SpringOptions, UiContext, prelude::column};
use std::time::Duration;

const DEFAULT_FPS: f64 = 30.;

/// One deterministic frame of a composition or sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub index: i64,
    pub count: u64,
    pub fps: f64,
    subframe: f64,
}

impl Frame {
    pub fn new(index: i64, count: u64, fps: f64) -> Self {
        Self {
            index,
            count,
            fps: if fps > 0. { fps } else { DEFAULT_FPS },
            subframe: 0.,
        }
    }

    pub(crate) fn sampled(index: i64, count: u64, fps: f64, subframe: f64) -> Self {
        Self {
            subframe,
            ..Self::new(index, count, fps)
        }
    }

    pub fn exact(self) -> f64 {
        self.index as f64 + self.subframe
    }

    pub fn time(self) -> Duration {
        Duration::from_secs_f64((self.exact() / self.fps).max(0.))
    }

    pub fn seconds(self) -> f64 {
        self.exact() / self.fps
    }

    pub fn progress(self) -> f64 {
        if self.count <= 1 {
            return 1.;
        }
        (self.exact() / (self.count - 1) as f64).clamp(0., 1.)
    }

    pub fn frames(self, duration: Duration) -> i64 {
        (duration.as_secs_f64() * self.fps).round() as i64
    }

    pub fn shift(self, from: i64, count: i64) -> (Self, bool) {
        let count = if count <= 0 {
            self.count.saturating_sub(from.max(0) as u64)
        } else {
            count as u64
        };
        let shifted = Self {
            index: self.index - from,
            count,
            ..self
        };
        let visible = shifted.index >= 0 && (shifted.index as u64) < count;
        (shifted, visible)
    }

    pub fn looped(self, count: u64) -> Self {
        assert!(count > 0, "render frame loop needs a positive count");
        let count_i64 = i64::try_from(count).expect("frame count exceeds i64");
        let mut index = self.index.rem_euclid(count_i64);
        if index == 0 && self.subframe < 0. && self.index != 0 {
            index = count_i64;
        }
        Self {
            index,
            count,
            ..self
        }
    }

    #[track_caller]
    pub fn sequence(
        self,
        ui: &mut UiContext<'_>,
        from: i64,
        count: i64,
        view: impl FnOnce(&mut UiContext<'_>, Frame),
    ) {
        let (frame, visible) = self.shift(from, count);
        let caller = std::panic::Location::caller();
        let mut key = plico::macro_support::StableHasher::new();
        key.write(b"plico-render.sequence");
        key.write(caller.file().as_bytes());
        key.write_u64(u64::from(caller.line()));
        key.write_u64(u64::from(caller.column()));
        ui.keyed(key.finish(), |ui| {
            let _ = column(ui)
                .absolute(0., 0.)
                .width_percent(100.)
                .height_percent(100.)
                + |ui| {
                    if visible {
                        view(ui, frame);
                    }
                };
        });
    }

    pub fn interpolate(self, input: &[f64], output: &[f64]) -> f64 {
        interpolate(self.exact(), input, output, Interpolation::default())
    }

    pub fn interpolate_with(self, input: &[f64], output: &[f64], options: Interpolation) -> f64 {
        interpolate(self.exact(), input, output, options)
    }

    pub fn spring(self, options: SpringOptions) -> f64 {
        options.progress(self.time())
    }

    pub fn spring_in(self, options: SpringOptions, frames: u64) -> f64 {
        if frames == 0 {
            return 1.;
        }
        let elapsed = options
            .settle_time()
            .mul_f64(self.exact().max(0.) / frames as f64);
        options.progress(elapsed)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Extrapolate {
    #[default]
    Clamp,
    Extend,
    Identity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interpolation {
    pub easing: Easing,
    pub left: Extrapolate,
    pub right: Extrapolate,
}

impl Default for Interpolation {
    fn default() -> Self {
        Self {
            easing: Easing::Linear,
            left: Extrapolate::Clamp,
            right: Extrapolate::Clamp,
        }
    }
}

pub fn interpolate(x: f64, input: &[f64], output: &[f64], options: Interpolation) -> f64 {
    assert!(
        input.len() >= 2 && input.len() == output.len(),
        "interpolate needs equal input and output slices with at least two values"
    );
    assert!(
        input.windows(2).all(|pair| pair[1] > pair[0]),
        "interpolation input values must increase"
    );
    if x < input[0] {
        match options.left {
            Extrapolate::Clamp => return output[0],
            Extrapolate::Identity => return x,
            Extrapolate::Extend => {}
        }
    }
    let last = input.len() - 1;
    if x > input[last] {
        match options.right {
            Extrapolate::Clamp => return output[last],
            Extrapolate::Identity => return x,
            Extrapolate::Extend => {}
        }
    }
    let mut segment = 0;
    while segment < last - 1 && x > input[segment + 1] {
        segment += 1;
    }
    let mut progress = (x - input[segment]) / (input[segment + 1] - input[segment]);
    if (0. ..=1.).contains(&progress) {
        progress = options.easing.sample(progress);
    }
    output[segment] + progress * (output[segment + 1] - output[segment])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_shift_loop_and_progress_follow_frame_position() {
        let frame = Frame::new(15, 31, 30.);
        assert_eq!(frame.time(), Duration::from_millis(500));
        assert_eq!(frame.progress(), 0.5);
        assert_eq!(frame.shift(10, 10).0.index, 5);
        assert!(frame.shift(10, 10).1);
        assert_eq!(Frame::new(-1, 30, 30.).looped(10).index, 9);
    }

    #[test]
    fn interpolation_clamps_extends_and_eases_each_segment() {
        let input = [0., 10., 20.];
        let output = [0., 100., 50.];
        assert_eq!(
            interpolate(-1., &input, &output, Interpolation::default()),
            0.
        );
        assert_eq!(
            interpolate(15., &input, &output, Interpolation::default()),
            75.
        );
        assert_eq!(
            interpolate(
                30.,
                &input,
                &output,
                Interpolation {
                    right: Extrapolate::Extend,
                    ..Interpolation::default()
                }
            ),
            0.
        );
        assert!(Frame::new(8, 30, 30.).spring(SpringOptions::BOUNCY) > 0.);
    }
}
