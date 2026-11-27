// Geometry channels follow MyGO ui/match.go's start/advance, MIT,
// revision 74c3f26d. See licenses/MyGo-MIT.txt.
use super::{Animation, PinnedRect, ValueChannel};
use crate::{Key, UiContext};
use std::time::Duration;

/// A shared element's clip and content in the same containing block.
/// Each rectangle is `[left, top, width, height]` in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedElementGeometry {
    pub clip: [f32; 4],
    pub content: [f32; 4],
}

impl SharedElementGeometry {
    /// An element whose content fills its clip.
    pub const fn rect(rect: [f32; 4]) -> Self {
        Self {
            clip: rect,
            content: rect,
        }
    }

    /// Center content of the given dimensions and scale it to cover the clip.
    pub fn cover(clip: [f32; 4], [width, height]: [f32; 2]) -> Self {
        let [x, y, w, h] = clip;
        let scale = (w / width).max(h / height);
        let (width, height) = (width * scale, height * scale);
        Self {
            clip,
            content: [x + (w - width) / 2., y + (h - height) / 2., width, height],
        }
    }

    /// Place the two rectangles with a stable layout origin. The content
    /// pose is local to the clip node, which must have no padding or border.
    pub fn pinned(self, rest: Self) -> Option<(PinnedRect, PinnedRect)> {
        let clip = PinnedRect::new(rest.clip, self.clip)?;
        let [x, y, w, h] = rest.content;
        let content = PinnedRect::new(
            [x - rest.clip[0], y - rest.clip[1], w, h],
            clip.to_local(self.content),
        )?;
        Some((clip, content))
    }

    fn axes(self) -> [f32; 8] {
        let [x, y, w, h] = self.clip;
        let [cx, cy, cw, ch] = self.content;
        [x, y, w, h, cx, cy, cw, ch]
    }

    fn from_axes([x, y, w, h, cx, cy, cw, ch]: [f32; 8]) -> Self {
        Self {
            clip: [x, y, w, h],
            content: [cx, cy, cw, ch],
        }
    }

    fn distance(self, other: Self) -> f32 {
        fn edges([x, y, w, h]: [f32; 4]) -> [f32; 4] {
            [x, y, x + w, y + h]
        }
        edges(self.clip)
            .into_iter()
            .chain(edges(self.content))
            .zip(edges(other.clip).into_iter().chain(edges(other.content)))
            .map(|(a, b)| (a - b).abs())
            .fold(0., f32::max)
    }

    fn valid(self) -> bool {
        self.axes().into_iter().all(f32::is_finite)
            && self.clip[2]
                .min(self.clip[3])
                .min(self.content[2])
                .min(self.content[3])
                > 0.
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedElementFrame {
    pub geometry: SharedElementGeometry,
    /// Largest visible or content edge displacement from the target.
    pub remaining: f32,
    pub active: bool,
}

/// Retained motion between two placements of an element with the same key.
/// Keep this in the scope that owns both placements. A new key starts at
/// `from`; an existing key retargets from its current pose and velocity.
#[derive(Default)]
pub struct SharedElementMotion {
    flight: Option<(Key, [ValueChannel; 8], SharedElementGeometry)>,
}

impl SharedElementMotion {
    /// Install the actual presentation after direct manipulation or a settled
    /// layout change. A later flight starts from this geometry.
    pub fn place(&mut self, key: impl Into<Key>, geometry: SharedElementGeometry, now: Duration) {
        let key = key.into();
        if self
            .flight
            .as_ref()
            .is_some_and(|(current, channels, previous)| {
                *current == key
                    && *previous == geometry
                    && !channels.iter().any(|channel| channel.active())
            })
        {
            return;
        }
        self.flight = geometry.valid().then(|| {
            (
                key,
                geometry.axes().map(|axis| ValueChannel::new(axis, now)),
                geometry,
            )
        });
    }

    /// Sample against the injected clock and request another frame while moving.
    pub fn sample(
        &mut self,
        cx: &mut UiContext<'_>,
        key: impl Into<Key>,
        from: SharedElementGeometry,
        target: SharedElementGeometry,
        animation: Animation,
    ) -> Option<SharedElementFrame> {
        animation.validate();
        let now = cx.now();
        let frame = self.sample_at(
            now,
            cx.display_preferences().reduce_motion,
            key.into(),
            from,
            target,
            animation,
        )?;
        if frame.active {
            cx.request_animation_frame();
        }
        Some(frame)
    }

    /// Frame sampling for a retained host with its own scheduling.
    #[doc(hidden)]
    pub fn sample_at(
        &mut self,
        now: Duration,
        reduce_motion: bool,
        key: Key,
        from: SharedElementGeometry,
        target: SharedElementGeometry,
        animation: Animation,
    ) -> Option<SharedElementFrame> {
        if !target.valid() {
            return None;
        }
        if self.flight.as_ref().is_none_or(|(old, ..)| *old != key) {
            self.place(key, if from.valid() { from } else { target }, now);
        }
        let (_, channels, previous) = self.flight.as_mut()?;
        let mut active = false;
        let mut axes = target.axes();
        for (channel, axis) in channels.iter_mut().zip(&mut axes) {
            let (value, moving) = channel.sample(now, *axis, animation, reduce_motion);
            *axis = value;
            active |= moving;
        }
        // Overshoot can temporarily drive an edge past its opposite edge.
        // Keep the last paintable box in that case until it recovers.
        let sampled = SharedElementGeometry::from_axes(axes);
        let geometry = if sampled.valid() { sampled } else { *previous };
        let remaining = geometry.distance(target);
        // MyGO's flying elements land once all edges are within a quarter point.
        // Keep an explicit delay and don't cut through at speed on a reversal.
        let landed = remaining < 0.25
            && channels.iter().all(|channel| channel.velocity().abs() < 8.)
            && animation.delay.is_zero();
        if !active || landed {
            self.place(key, target, now);
            return Some(SharedElementFrame {
                geometry: target,
                remaining: 0.,
                active: false,
            });
        }
        *previous = geometry;
        Some(SharedElementFrame {
            geometry,
            remaining,
            active: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_keeps_current_geometry_and_momentum_then_lands_exactly() {
        let tile = SharedElementGeometry::cover([40., 60., 150., 120.], [200., 400.]);
        let detail = SharedElementGeometry::rect([370., 90., 250., 500.]);
        let key = Key::Number(1);
        let mut motion = SharedElementMotion::default();
        let advance = |motion: &mut SharedElementMotion, ms, from, to| {
            motion
                .sample_at(
                    Duration::from_millis(ms),
                    false,
                    key,
                    from,
                    to,
                    Animation::SMOOTH,
                )
                .unwrap()
        };
        assert_eq!(advance(&mut motion, 0, tile, detail).geometry, tile);
        let opening = advance(&mut motion, 80, tile, detail);
        assert!(opening.geometry.clip[0] > tile.clip[0]);
        let reversing = advance(&mut motion, 80, detail, tile);
        assert_eq!(reversing.geometry, opening.geometry);
        let first = advance(&mut motion, 81, detail, tile);
        assert!(first.geometry.clip[0] > reversing.geometry.clip[0]);
        let landed = advance(&mut motion, 1_600, detail, tile);
        assert_eq!(landed.geometry, tile);
        assert_eq!(landed.remaining, 0.);
        assert!(!landed.active);
    }

    #[test]
    fn keys_and_reduced_motion_select_the_intended_placement() {
        let tile = SharedElementGeometry::rect([12., 30., 60., 60.]);
        let detail = SharedElementGeometry::rect([120., 80., 300., 300.]);
        let mut motion = SharedElementMotion::default();
        let now = Duration::ZERO;
        motion.place(1_u64, detail, now);
        let new = motion
            .sample_at(now, false, 2_u64.into(), tile, detail, Animation::SMOOTH)
            .unwrap();
        assert_eq!(new.geometry, tile);
        let reduced = motion
            .sample_at(now, true, 2_u64.into(), tile, detail, Animation::SMOOTH)
            .unwrap();
        assert_eq!(reduced.geometry, detail);
        assert!(!reduced.active);
        let (clip, content) = tile.pinned(detail).unwrap();
        assert_eq!(clip.layout_size(), [300., 300.]);
        assert_eq!(content.layout_size(), [300., 300.]);
    }
}
