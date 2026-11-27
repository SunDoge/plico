//! Retained optical motion for anchored glass. The host clock is the only time source.
use std::time::Duration;

// The centered and stretched trajectories follow Liquid Glass UI's glass-popup.tsx (MIT).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct PopupMorph {
    last: Option<std::time::Duration>,
    waiting: f32,
    open: bool,
    lead: plico::OpticalSpring,
    cross: plico::OpticalSpring,
    calm: plico::OpticalSpring,
    drop: plico::OpticalSpring,
    recoil: plico::OpticalSpring,
    pub(super) content: f32,
}

pub(super) fn smoothstep(start: f32, end: f32, value: f32) -> f32 {
    let t = ((value - start) / (end - start)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

impl PopupMorph {
    pub(super) fn sample(&mut self, now: std::time::Duration, open: bool) -> bool {
        let dt = self
            .last
            .replace(now)
            .map_or(0., |last| now.saturating_sub(last).as_secs_f32());
        let was_open = self.open;
        self.open = open;
        // Advance the previous direction up to the input timestamp.
        let moving = if was_open {
            let before = self.waiting;
            self.waiting += dt;
            let grow_dt = (self.waiting - 0.1).max(0.) - (before - 0.1).max(0.);
            let drop_dt = (self.waiting - 0.2).max(0.) - (before - 0.2).max(0.);
            let mut moving = self.waiting < 0.2;
            if grow_dt > 0. {
                moving |= self.lead.step(1., 30., 1., grow_dt);
                moving |= self.cross.step(1., 16., 0.58, grow_dt);
                moving |= self.calm.step(1., 12., 1., grow_dt);
            }
            if drop_dt > 0. {
                moving |= self.drop.step(1., 9., 1., drop_dt);
            }
            moving |= self.recoil.step(0., 17.5 / 8_f32.sqrt(), 0.6, dt);
            self.content = smoothstep(0., 0.75, self.calm.x);
            moving
        } else {
            let mut moving = self.lead.step(0., 14., 1., dt);
            moving |= self.cross.step(0., 14., 1., dt);
            moving |= self.calm.step(0., 14., 1., dt);
            moving |= self.drop.step(0., 14., 1., dt);
            moving |= self.recoil.step(0., 14., 1., dt);
            self.content =
                (self.content - dt / 0.12)
                    .max(0.)
                    .min(smoothstep(0., 0.75, self.calm.x));
            moving
        };
        if open && !was_open {
            if self.lead.x > 0.02 {
                self.waiting = 0.2;
            } else {
                self.waiting = 0.;
                self.recoil.velocity += 1.;
            }
        }
        if !open
            && self.lead.x.max(self.cross.x).max(self.calm.x) < 0.004
            && self.recoil.x.abs() < 0.006
        {
            *self = Self {
                last: Some(now),
                ..Self::default()
            };
            return false;
        }
        moving || open != was_open
    }

    pub(super) fn frame(&self, from_top: bool) -> [f32; 5] {
        let lead = self.lead.x.clamp(0., 1.);
        let direction = if from_top { 1. } else { -1. };
        let throw = direction * 80. * (self.recoil.x * 128.).round() / 128.;
        let carry = -direction * 60. * (lead - self.drop.x.clamp(0., lead));
        [
            lead,
            self.cross.x.clamp(0., 1.15),
            self.calm.x.clamp(0., 1.),
            throw,
            carry,
        ]
    }
}

#[derive(Default)]
pub(super) struct PopupList {
    last: Option<Duration>,
    open: bool,
    time: f32,
}

impl PopupList {
    const END: f32 = 0.82;

    pub(super) fn sample(&mut self, now: Duration, open: bool) -> bool {
        let dt = self
            .last
            .replace(now)
            .map_or(0., |last| now.saturating_sub(last).as_secs_f32());
        let pace = if self.open { 1. } else { -26. / 11. };
        self.time = (self.time + dt * pace).clamp(0., Self::END);
        self.open = open;
        if open {
            self.time < Self::END
        } else {
            self.time > 0.
        }
    }

    pub(super) fn frame(&self) -> [f32; 3] {
        let time = self.time;
        let critical = |rate: f32, t: f32| {
            let phase = rate * t;
            1. - (1. + phase) * (-phase).exp()
        };
        let opening = if time == Self::END {
            1.
        } else {
            critical(11., time)
        };
        // Preserve the neck's upward velocity when switching from attachment
        // to the release spring. Otherwise it immediately flattens after the click.
        const DETACH: f32 = 0.048;
        let attach = critical(28., time.min(DETACH));
        let released = (time - DETACH).max(0.);
        let attach_velocity = 28_f32.powi(2) * DETACH * (-28. * DETACH).exp();
        let bridge =
            (attach + (attach_velocity + 5.6 * attach) * released) * (-5.6 * released).exp();
        let release_tail = 1. - ((time - 0.72) / (Self::END - 0.72)).clamp(0., 1.);
        let bridge = bridge * release_tail;
        let reveal = (time - 0.307).max(0.);
        [opening, bridge, reveal]
    }
}

pub(super) enum PopupMotion {
    Morph(PopupMorph),
    List(PopupList),
}

impl PopupMotion {
    pub(super) fn new(centered: bool) -> Self {
        if centered {
            Self::Morph(PopupMorph::default())
        } else {
            Self::List(PopupList::default())
        }
    }
    pub(super) fn sample(&mut self, now: Duration, open: bool) -> bool {
        match self {
            Self::Morph(morph) => morph.sample(now, open),
            Self::List(list) => list.sample(now, open),
        }
    }

    pub(super) fn open_at_rest(centered: bool, now: Duration) -> Self {
        if centered {
            let settled = plico::OpticalSpring {
                x: 1.,
                velocity: 0.,
            };
            Self::Morph(PopupMorph {
                last: Some(now),
                waiting: 0.2,
                open: true,
                lead: settled,
                cross: settled,
                calm: settled,
                drop: settled,
                content: 1.,
                ..PopupMorph::default()
            })
        } else {
            Self::List(PopupList {
                last: Some(now),
                open: true,
                time: PopupList::END,
            })
        }
    }
}
