//! Declaration-owned scalar motion, following MyGO's channel model.
//! Source: MyGO `6bd276dda679449336f68277eaaa988e1fadc54b`,
//! `ui/widgets.go` and `ui/motion.go`, MIT; see `licenses/MyGo-MIT.txt`.
use crate::Easing;
use std::time::Duration;

mod content;
mod placement;
pub use content::{ContentTransition, ContentTransitionFrame};
pub use placement::PinnedRect;

/// How [`crate::UiContext::animate_to`] moves a scalar to its next target.
/// A positive `duration` selects an easing curve; zero selects a spring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    pub duration: Duration,
    pub ease: Easing,
    pub spring: SpringOptions,
    pub delay: Duration,
    /// Continue non-spatial effects such as opacity while reduced motion is on.
    pub non_motion: bool,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            duration: Duration::ZERO,
            ease: Easing::EaseOut,
            spring: SpringOptions::default(),
            delay: Duration::ZERO,
            non_motion: false,
        }
    }
}

impl Animation {
    pub const SMOOTH: Self = Self {
        duration: Duration::ZERO,
        ease: Easing::EaseOut,
        spring: SpringOptions::SMOOTH,
        delay: Duration::ZERO,
        non_motion: false,
    };
    pub const SNAPPY: Self = Self {
        spring: SpringOptions::SNAPPY,
        ..Self::SMOOTH
    };
    pub const BOUNCY: Self = Self {
        spring: SpringOptions::BOUNCY,
        ..Self::SMOOTH
    };

    pub const fn tween(duration: Duration, ease: Easing) -> Self {
        Self {
            duration,
            ease,
            ..Self::SMOOTH
        }
    }

    pub(crate) fn validate(self) {
        if self.duration.is_zero() {
            self.spring.physics();
        }
    }
}

/// Perceptual or physical parameters for an analytic damped spring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringOptions {
    /// Perceived undamped period. Defaults to 500 ms when zero.
    pub duration: Duration,
    /// Overshoot in `(-1, 1)`: zero settles without overshoot.
    pub bounce: f64,
    /// Physical mass. Zero means 1.
    pub mass: f64,
    /// Physical stiffness. Set with `damping`, not `duration`/`bounce`.
    pub stiffness: f64,
    /// Physical damping. Set with `stiffness`, not `duration`/`bounce`.
    pub damping: f64,
    /// Resting displacement in value units. Zero means 0.001.
    pub precision: f64,
    /// Explicit velocity in value units per second; zero retains momentum.
    pub velocity: f32,
}

impl SpringOptions {
    pub const SMOOTH: Self = Self {
        duration: Duration::from_millis(500),
        bounce: 0.,
        mass: 0.,
        stiffness: 0.,
        damping: 0.,
        precision: 0.,
        velocity: 0.,
    };
    pub const SNAPPY: Self = Self {
        bounce: 0.15,
        ..Self::SMOOTH
    };
    pub const BOUNCY: Self = Self {
        bounce: 0.3,
        ..Self::SMOOTH
    };

    /// Resolve perceptual settings to physical parameters for the shared solver.
    ///
    /// Panics if any option is invalid or both parameterizations are supplied.
    /// The initial velocity remains a property of the caller's animation state.
    pub fn physics(self) -> SpringPhysics {
        for value in [
            self.bounce,
            self.mass,
            self.stiffness,
            self.damping,
            self.precision,
            f64::from(self.velocity),
        ] {
            assert!(value.is_finite(), "spring values must be finite");
        }
        assert!(
            self.mass >= 0. && self.stiffness >= 0. && self.damping >= 0. && self.precision >= 0.,
            "spring values must not be negative"
        );
        let defaults = SpringPhysics::default();
        let mass = if self.mass == 0. {
            defaults.mass
        } else {
            self.mass
        };
        let precision = if self.precision == 0. {
            defaults.precision
        } else {
            self.precision
        };
        let (stiffness, damping) = if self.stiffness != 0. || self.damping != 0. {
            assert!(
                self.duration.is_zero() && self.bounce == 0.,
                "a spring takes duration/bounce or stiffness/damping, not both"
            );
            (
                if self.stiffness == 0. {
                    defaults.stiffness
                } else {
                    self.stiffness
                },
                if self.damping == 0. {
                    defaults.damping
                } else {
                    self.damping
                },
            )
        } else {
            assert!(
                self.bounce > -1. && self.bounce < 1.,
                "spring bounce must be between -1 and 1"
            );
            let duration = if self.duration.is_zero() {
                Duration::from_millis(500)
            } else {
                self.duration
            };
            let frequency = 2. * std::f64::consts::PI / duration.as_secs_f64();
            let ratio = if self.bounce < 0. {
                1. / (1. + self.bounce)
            } else {
                1. - self.bounce
            };
            (frequency * frequency * mass, 2. * ratio * frequency * mass)
        };
        assert!(
            stiffness.is_finite() && stiffness > 0. && damping.is_finite() && damping > 0.,
            "spring values exceed numerical range"
        );
        SpringPhysics {
            mass,
            stiffness,
            damping,
            precision,
        }
    }

    /// Sample a normalized spring moving from zero to one after `elapsed`.
    ///
    /// The result may overshoot one. It follows the same analytic trajectory
    /// as [`UiContext::spring`](crate::UiContext::spring), but has no retained
    /// channel, which makes it suitable for deterministic media frames.
    pub fn progress(self, elapsed: Duration) -> f64 {
        if elapsed.is_zero() {
            return 0.;
        }
        let spring = self.physics();
        let (displacement, _) = spring.sample(-1., f64::from(self.velocity), elapsed.as_secs_f64());
        1. + displacement
    }

    /// Time for a normalized spring to satisfy the runtime's rest thresholds.
    /// The search is deterministic to one millisecond and bounded to one minute.
    pub fn settle_time(self) -> Duration {
        let spring = self.physics();
        let mut displacement = -1.;
        let mut velocity = f64::from(self.velocity);
        for millis in 1..=60_000 {
            (displacement, velocity) = spring.sample(displacement, velocity, 0.001);
            if spring.at_rest(displacement, velocity) {
                return Duration::from_millis(millis);
            }
        }
        Duration::from_secs(60)
    }
}

impl Default for SpringOptions {
    fn default() -> Self {
        Self {
            duration: Duration::ZERO,
            bounce: 0.,
            mass: 0.,
            stiffness: 0.,
            damping: 0.,
            precision: 0.,
            velocity: 0.,
        }
    }
}

/// Physical coefficients of an analytic damped oscillator.
///
/// Pass coefficients directly to retained motion, or use
/// [`SpringOptions::physics`] to resolve perceptual settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringPhysics {
    /// Moving mass. Must be positive and finite.
    pub mass: f64,
    /// Restoring force coefficient. Must be positive and finite.
    pub stiffness: f64,
    /// Damping force coefficient. Must be non-negative and finite.
    pub damping: f64,
    /// Maximum displacement from the target before settling.
    pub precision: f64,
}

impl Default for SpringPhysics {
    fn default() -> Self {
        Self {
            mass: 1.,
            stiffness: 170.,
            damping: 26.,
            precision: 0.001,
        }
    }
}

impl SpringPhysics {
    /// Shared rest criterion for both declaration and renderer motion.
    pub fn at_rest(self, displacement: f64, velocity: f64) -> bool {
        displacement.abs() <= self.precision
            && velocity.abs() <= self.precision * (self.stiffness / self.mass).sqrt()
    }
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    target: f64,
    at: Duration,
    animation: Animation,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ValueChannel {
    value: f64,
    velocity: f64,
    target: f64,
    at: Duration,
    moving: bool,
    curve: bool,
    from: f64,
    started_at: Duration,
    duration: Duration,
    ease: Easing,
    spring: SpringPhysics,
    pending: Option<Pending>,
    jumped: bool,
}

impl ValueChannel {
    pub(crate) fn new(value: f32, now: Duration) -> Self {
        Self {
            value: f64::from(value),
            velocity: 0.,
            target: f64::from(value),
            at: now,
            moving: false,
            curve: false,
            from: f64::from(value),
            started_at: now,
            duration: Duration::ZERO,
            ease: Easing::EaseOut,
            spring: SpringOptions::default().physics(),
            pending: None,
            jumped: false,
        }
    }

    pub(crate) fn sample(
        &mut self,
        now: Duration,
        target: f32,
        animation: Animation,
        reduce_motion: bool,
    ) -> (f32, bool) {
        let target = f64::from(target);
        self.advance(now);
        if reduce_motion && !animation.non_motion {
            self.settle(now, target);
            return (target as f32, false);
        }
        if target != self.goal() {
            if animation.delay.is_zero() {
                self.pending = None;
                self.retarget(target, animation);
            } else {
                self.pending = Some(Pending {
                    target,
                    at: now.saturating_add(animation.delay),
                    animation: Animation {
                        delay: Duration::ZERO,
                        ..animation
                    },
                });
            }
        } else if self.moving && !self.curve && animation.duration.is_zero() {
            self.spring = animation.spring.physics();
        }
        (self.value as f32, self.moving || self.pending.is_some())
    }

    pub(crate) fn jump(&mut self, now: Duration, value: f32) -> f32 {
        let value = f64::from(value);
        let elapsed = now.saturating_sub(self.at).as_secs_f64();
        if elapsed > 0. && elapsed <= 0.1 && self.jumped {
            self.velocity = (value - self.value) / elapsed;
        } else if elapsed > 0. {
            self.advance(now);
            if elapsed > 0.1 {
                self.velocity = 0.;
            }
        }
        self.value = value;
        self.target = value;
        self.moving = false;
        self.pending = None;
        self.jumped = true;
        self.at = now;
        value as f32
    }

    pub(crate) fn active(self) -> bool {
        self.moving || self.pending.is_some()
    }

    pub(crate) fn velocity(self) -> f32 {
        self.velocity as f32
    }

    fn goal(self) -> f64 {
        self.pending.map_or(self.target, |pending| pending.target)
    }

    fn settle(&mut self, now: Duration, target: f64) {
        self.value = target;
        self.velocity = 0.;
        self.target = target;
        self.at = now;
        self.moving = false;
        self.pending = None;
        self.jumped = false;
    }

    fn retarget(&mut self, target: f64, animation: Animation) {
        self.target = target;
        self.moving = true;
        self.jumped = false;
        if animation.duration.is_zero() {
            self.curve = false;
            self.spring = animation.spring.physics();
            if animation.spring.velocity != 0. {
                self.velocity = f64::from(animation.spring.velocity);
            }
        } else {
            self.curve = true;
            self.from = self.value;
            self.started_at = self.at;
            self.duration = animation.duration;
            self.ease = animation.ease;
        }
    }

    fn advance(&mut self, now: Duration) {
        if let Some(pending) = self.pending
            && now >= pending.at
        {
            self.step(pending.at);
            self.pending = None;
            self.retarget(pending.target, pending.animation);
        }
        self.step(now);
    }

    fn step(&mut self, now: Duration) {
        if now <= self.at {
            self.at = now;
            return;
        }
        let elapsed = now.saturating_sub(self.at).as_secs_f64();
        self.at = now;
        if !self.moving {
            return;
        }
        if self.curve {
            let total = self.duration.as_secs_f64();
            let progress = now.saturating_sub(self.started_at).as_secs_f64() / total;
            if progress >= 1. {
                self.value = self.target;
                self.velocity = 0.;
                self.moving = false;
                return;
            }
            let span = self.target - self.from;
            self.value = self.from + span * self.ease.sample(progress);
            self.velocity = span * self.ease.slope(progress) / total;
            return;
        }
        let (displacement, velocity) =
            self.spring
                .sample(self.value - self.target, self.velocity, elapsed);
        self.value = self.target + displacement;
        self.velocity = velocity;
        if self.spring.at_rest(displacement, velocity) {
            self.value = self.target;
            self.velocity = 0.;
            self.moving = false;
        }
    }
}

impl SpringPhysics {
    /// Sample a damped oscillator with already validated physical parameters.
    pub fn sample(self, x: f64, velocity: f64, dt: f64) -> (f64, f64) {
        let Self {
            mass,
            stiffness,
            damping,
            ..
        } = self;
        if x == 0. && velocity == 0. {
            return (0., 0.);
        }
        let frequency = (stiffness / mass).sqrt();
        let decay = (damping / mass) / 2.;
        let discriminant = decay * decay - frequency * frequency;
        if discriminant.abs() < 1e-8 * frequency * frequency {
            let b = velocity + decay * x;
            let envelope = (-decay * dt).exp();
            return (
                (x + b * dt) * envelope,
                (velocity - decay * b * dt) * envelope,
            );
        }
        if discriminant < 0. {
            let damped = (-discriminant).sqrt();
            let (sin, cos) = (damped * dt).sin_cos();
            let b = (velocity + decay * x) / damped;
            let envelope = (-decay * dt).exp();
            let value = x * cos + b * sin;
            return (
                value * envelope,
                (damped * (-x * sin + b * cos) - decay * value) * envelope,
            );
        }
        let root = discriminant.sqrt();
        let slow = -frequency * frequency / (decay + root);
        let fast = -decay - root;
        let slow_weight = (velocity - fast * x) / (slow - fast);
        let fast_weight = x - slow_weight;
        let slow_term = slow_weight * (slow * dt).exp();
        let fast_term = fast_weight * (fast * dt).exp();
        (slow_term + fast_term, slow * slow_term + fast * fast_term)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_spring_is_independent_of_frame_partitioning() {
        let mut one = ValueChannel::new(0., Duration::ZERO);
        let mut split = one;
        one.sample(Duration::ZERO, 100., Animation::BOUNCY, false);
        split.sample(Duration::ZERO, 100., Animation::BOUNCY, false);
        let direct = one
            .sample(Duration::from_millis(360), 100., Animation::BOUNCY, false)
            .0;
        split.sample(Duration::from_millis(120), 100., Animation::BOUNCY, false);
        split.sample(Duration::from_millis(240), 100., Animation::BOUNCY, false);
        let partitioned = split
            .sample(Duration::from_millis(360), 100., Animation::BOUNCY, false)
            .0;
        assert!(
            (direct - partitioned).abs() < 1e-4,
            "{direct} {partitioned}"
        );
        assert!(direct > 100., "bouncy preset should overshoot: {direct}");
    }

    #[test]
    fn public_spring_sampling_matches_the_retained_channel() {
        let options = SpringOptions::BOUNCY;
        let elapsed = Duration::from_millis(360);
        let mut channel = ValueChannel::new(0., Duration::ZERO);
        channel.sample(Duration::ZERO, 1., Animation::BOUNCY, false);
        let retained = channel.sample(elapsed, 1., Animation::BOUNCY, false).0;
        let sampled = options.progress(elapsed);
        assert!((f64::from(retained) - sampled).abs() < 1e-6);
        let settled = options.settle_time();
        assert!(settled > Duration::ZERO && settled <= Duration::from_secs(60));
        assert!((options.progress(settled) - 1.).abs() <= 0.002);
    }

    #[test]
    #[should_panic(expected = "duration/bounce or stiffness/damping")]
    fn spring_rejects_mixed_perceptual_and_physical_parameters() {
        SpringOptions {
            duration: Duration::from_millis(300),
            stiffness: 170.,
            ..Default::default()
        }
        .physics();
    }
}

// Analytic optical spring adapted from Liquid Glass UI (MIT).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[doc(hidden)]
pub struct OpticalSpring {
    pub x: f32,
    pub velocity: f32,
}

impl OpticalSpring {
    pub fn step(&mut self, target: f32, omega: f32, damping: f32, dt: f32) -> bool {
        let delta = self.x - target;
        if damping >= 1. {
            let b = self.velocity + omega * delta;
            let decay = (-omega * dt).exp();
            let x = (delta + b * dt) * decay;
            self.x = target + x;
            self.velocity = (self.velocity - omega * b * dt) * decay;
            return self.settle(target);
        }
        let wd = omega * (1. - damping * damping).sqrt();
        let b = (self.velocity + damping * omega * delta) / wd;
        let decay = (-damping * omega * dt).exp();
        let (sin, cos) = (wd * dt).sin_cos();
        let x = (delta * cos + b * sin) * decay;
        self.x = target + x;
        self.velocity = decay * wd * (b * cos - delta * sin) - damping * omega * x;
        self.settle(target)
    }

    fn settle(&mut self, target: f32) -> bool {
        if (self.x - target).abs() < 0.0005 && self.velocity.abs() < 0.005 {
            self.x = target;
            self.velocity = 0.;
            return false;
        }
        true
    }
}
