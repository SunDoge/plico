//! Declaration-owned scalar motion, following MyGO's channel model.
//! Source: MyGO `6bd276dda679449336f68277eaaa988e1fadc54b`,
//! `ui/widgets.go` and `ui/motion.go`, MIT; see `licenses/MyGo-MIT.txt`.
use crate::Easing;
use std::time::Duration;

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

    fn physics(self) -> SpringPhysics {
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
        let mass = if self.mass == 0. { 1. } else { self.mass };
        let precision = if self.precision == 0. {
            0.001
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
                    170.
                } else {
                    self.stiffness
                },
                if self.damping == 0. {
                    26.
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

#[derive(Clone, Copy, Debug)]
struct SpringPhysics {
    mass: f64,
    stiffness: f64,
    damping: f64,
    precision: f64,
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
        let displacement = self.value - self.target;
        let (displacement, velocity) = solve_spring(
            displacement,
            self.velocity,
            self.spring.mass,
            self.spring.stiffness,
            self.spring.damping,
            elapsed,
        );
        self.value = self.target + displacement;
        self.velocity = velocity;
        let speed_precision =
            self.spring.precision * (self.spring.stiffness / self.spring.mass).sqrt();
        if displacement.abs() <= self.spring.precision && velocity.abs() <= speed_precision {
            self.value = self.target;
            self.velocity = 0.;
            self.moving = false;
        }
    }
}

fn solve_spring(
    x: f64,
    velocity: f64,
    mass: f64,
    stiffness: f64,
    damping: f64,
    dt: f64,
) -> (f64, f64) {
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
