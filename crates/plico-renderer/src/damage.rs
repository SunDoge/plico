//! Optional redraw information, independent of layout and window APIs.
use vello_common::kurbo::Rect;

/// Damage in physical surface pixels, relative to the last successful draw.
/// Consumers may always promote a region to `Full`. Producers must include
/// both old and new ink bounds, including effects and overlapping content.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DamageRegion {
    /// Contents or preservation are unknown; replace the entire surface.
    #[default]
    Full,
    /// The previous surface contents remain valid without drawing.
    Empty,
    /// Redraw this rectangle (consumers round outwards to pixel boundaries).
    Rect(Rect),
}

impl DamageRegion {
    /// Whether two conservative regions overlap. Unknown/full bounds cannot
    /// be culled; an empty region never intersects anything.
    pub fn intersects(self, other: Self) -> bool {
        match (self, other) {
            (Self::Empty, _) | (_, Self::Empty) => false,
            (Self::Full, _) | (_, Self::Full) => true,
            (Self::Rect(a), Self::Rect(b)) => {
                let r = a.intersect(b);
                r.width() > 0. && r.height() > 0.
            }
        }
    }
    /// Fraction of a surface covered after clipping; zero for an empty surface.
    pub fn coverage(self, width: u32, height: u32) -> f64 {
        if width == 0 || height == 0 {
            return 0.;
        }
        match self.for_surface(width, height) {
            Self::Full => 1.,
            Self::Empty => 0.,
            Self::Rect(r) => r.area() / (f64::from(width) * f64::from(height)),
        }
    }
    /// Round outwards and intersect the surface. Invalid geometry falls back
    /// to full damage; a finite empty or offscreen rectangle becomes empty.
    pub fn for_surface(self, width: u32, height: u32) -> Self {
        match self {
            Self::Rect(r) => {
                if ![r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()) {
                    return Self::Full;
                }
                if r.x0 >= r.x1 || r.y0 >= r.y1 {
                    return Self::Empty;
                }
                let r = Rect::new(r.x0.floor(), r.y0.floor(), r.x1.ceil(), r.y1.ceil())
                    .intersect(Rect::new(0., 0., width as f64, height as f64));
                if r.width() <= 0. || r.height() <= 0. {
                    Self::Empty
                } else {
                    Self::Rect(r)
                }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_rounds_outwards_clamps_and_rejects_nonfinite_geometry() {
        assert_eq!(
            DamageRegion::Rect(Rect::new(-2.2, 1.1, 8.1, 12.)).for_surface(10, 10),
            DamageRegion::Rect(Rect::new(0., 1., 9., 10.))
        );
        assert_eq!(
            DamageRegion::Rect(Rect::new(12., 0., 14., 1.)).for_surface(10, 10),
            DamageRegion::Empty
        );
        assert_eq!(
            DamageRegion::Rect(Rect::new(2., 0., 1., 1.)).for_surface(10, 10),
            DamageRegion::Empty
        );
        assert_eq!(
            DamageRegion::Rect(Rect::new(0., 0., f64::INFINITY, 1.)).for_surface(10, 10),
            DamageRegion::Full
        );
    }
}

/// Optional command-stream damage producer, independent of UI identity/layout.
///
/// Keep one tracker per preserved render target. `update` compares against that
/// target's previous commands, including removed commands and changed scopes.
/// The backend supplies conservative ink bounds for leaf commands; unknown
/// bounds use their enclosing clip, or full damage when there is no clip.
/// Call [`Self::invalidate`] after any failed draw/submission or content loss.
#[derive(Default)]
pub struct SceneDamageTracker {
    previous: Option<crate::Scene>,
    bounds: Vec<DamageRegion>,
    contexts: Vec<Option<Rect>>,
    nonlocal: bool,
    size: Option<(u32, u32)>,
}
impl DamageRegion {
    /// Conservative union; full damage dominates and empty damage is neutral.
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::Empty, r) | (r, Self::Empty) => r,
            (Self::Rect(a), Self::Rect(b)) => Self::Rect(a.union(b)),
        }
    }
}
impl SceneDamageTracker {
    /// Conservative bounds of a command from the last [`Self::update`].
    /// Consumers must use the exact command stream passed to that update.
    /// Missing entries remain unknown and must not be culled.
    pub fn command_bounds(&self, index: usize) -> DamageRegion {
        self.bounds
            .get(index)
            .copied()
            .unwrap_or(DamageRegion::Full)
    }
    /// Forget target contents; the next update reports full damage.
    pub fn invalidate(&mut self) {
        self.previous = None;
        self.bounds.clear();
        self.contexts.clear();
        self.nonlocal = false;
        self.size = None;
    }

    /// Estimate conservative damage from the target's last command stream.
    /// `ink` returns physical-pixel ink bounds *after* the command transform.
    /// Return `None` for unknown bounds. The tracker adds a one-pixel guard
    /// around bounds for raster edge coverage and intersects enclosing clips.
    /// Filters and custom shaders force full damage because they may depend on
    /// pixels outside their own output or on time not reflected in commands.
    pub fn update(
        &mut self,
        scene: &crate::Scene,
        width: u32,
        height: u32,
        mut ink: impl FnMut(&crate::PaintCommand) -> Option<Rect>,
    ) -> DamageRegion {
        use crate::PaintCommand;
        use vello_common::{kurbo::Shape, peniko::BlendMode};
        let mut damage =
            if self.previous.is_none() || self.size != Some((width, height)) || self.nonlocal {
                DamageRegion::Full
            } else {
                DamageRegion::Empty
            };
        self.nonlocal = false;
        let mut clips: Vec<Option<Rect>> = Vec::new();
        let mut clip: Option<Rect> = None;
        for (index, command) in scene.commands.iter().enumerate() {
            let context = clip;
            let same_command =
                self.previous.as_ref().and_then(|s| s.commands.get(index)) == Some(command);
            let bounds = match command {
                PaintCommand::PushLayer {
                    transform,
                    clip: path,
                    filter,
                    blend,
                    ..
                } => {
                    if filter.is_some() || *blend != BlendMode::default() {
                        damage = DamageRegion::Full;
                        self.nonlocal = true;
                    }
                    clips.push(clip);
                    let r = transform.transform_rect_bbox(path.bounding_box());
                    clip = Some(clip.map_or(r, |outer| outer.intersect(r)));
                    clip_region(clip)
                }
                PaintCommand::PushClip {
                    transform,
                    clip: path,
                } => {
                    clips.push(clip);
                    let r = transform.transform_rect_bbox(path.bounding_box());
                    clip = Some(clip.map_or(r, |outer| outer.intersect(r)));
                    clip_region(clip)
                }
                PaintCommand::PopLayer => {
                    let bounds = clip_region(clip);
                    match clips.pop() {
                        Some(parent) => clip = parent,
                        None => damage = DamageRegion::Full,
                    }
                    bounds
                }
                PaintCommand::Shader { .. } => {
                    self.nonlocal = true;
                    damage = DamageRegion::Full;
                    DamageRegion::Full
                }
                _ if same_command
                    && self.size == Some((width, height))
                    && self.contexts.get(index) == Some(&context) =>
                {
                    self.bounds[index]
                }
                _ => match ink(command) {
                    Some(r) if finite(r) => {
                        let r = r.inflate(1., 1.);
                        clip_region(Some(clip.map_or(r, |outer| r.intersect(outer))))
                    }
                    Some(_) => DamageRegion::Full,
                    None => clip_region(clip),
                },
            }
            .for_surface(width, height);
            let changed = !same_command || self.bounds.get(index) != Some(&bounds);
            if changed {
                damage = damage.union(bounds);
                if let Some(old) = self.bounds.get(index) {
                    damage = damage.union(*old);
                }
            }
            if index < self.bounds.len() {
                self.contexts[index] = context;
                self.bounds[index] = bounds;
            } else {
                self.bounds.push(bounds);
                self.contexts.push(context);
            }
        }
        for removed in &self.bounds[scene.commands.len()..] {
            damage = damage.union(*removed);
        }
        self.bounds.truncate(scene.commands.len());
        self.contexts.truncate(scene.commands.len());
        if !clips.is_empty() {
            damage = DamageRegion::Full;
        }
        self.previous = Some(scene.clone());
        self.size = Some((width, height));
        damage.for_surface(width, height)
    }
}
fn finite(r: Rect) -> bool {
    [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())
}
fn clip_region(clip: Option<Rect>) -> DamageRegion {
    match clip {
        Some(r) if !finite(r) => DamageRegion::Full,
        Some(r) if r.width() <= 0. || r.height() <= 0. => DamageRegion::Empty,
        Some(r) => DamageRegion::Rect(r.inflate(1., 1.)),
        None => DamageRegion::Full,
    }
}

#[cfg(test)]
mod tracker_tests {
    use super::*;
    use crate::{PaintCommand, PaintScene, Scene};
    use vello_common::{
        kurbo::{Affine, Shape},
        peniko::{Color, Fill},
    };
    fn bounds(command: &PaintCommand) -> Option<Rect> {
        match command {
            PaintCommand::Fill {
                shape, transform, ..
            } => Some(transform.transform_rect_bbox(shape.bounding_box())),
            _ => None,
        }
    }
    fn scene(x: f64) -> Scene {
        let mut s = Scene::new();
        s.fill(
            Fill::NonZero,
            Affine::translate((x, 0.)),
            Color::BLACK,
            None,
            &Rect::new(0., 10., 10., 20.),
        );
        s
    }
    #[test]
    fn tracks_old_new_removed_bounds_and_reuses_unchanged_ink() {
        let mut tracker = SceneDamageTracker::default();
        assert_eq!(
            tracker.update(&scene(10.), 100, 100, bounds),
            DamageRegion::Full
        );
        assert_eq!(
            tracker.update(&scene(10.), 100, 100, |_| panic!(
                "unchanged ink must be cached"
            )),
            DamageRegion::Empty
        );
        let DamageRegion::Rect(r) = tracker.update(&scene(40.), 100, 100, bounds) else {
            panic!()
        };
        assert!(r.x0 <= 10. && r.x1 >= 50.);
        let DamageRegion::Rect(r) = tracker.update(&Scene::new(), 100, 100, bounds) else {
            panic!()
        };
        assert!(r.x0 <= 40. && r.x1 >= 50.);
        tracker.invalidate();
        assert_eq!(
            tracker.update(&Scene::new(), 100, 100, bounds),
            DamageRegion::Full
        );
    }
    #[test]
    fn resized_and_filtered_frames_invalidate_including_filter_removal() {
        let mut tracker = SceneDamageTracker::default();
        let mut s = scene(10.);
        s.push_layer(
            vello_common::peniko::BlendMode::default(),
            1.,
            Affine::IDENTITY,
            &Rect::new(10., 10., 20., 20.),
            Some(std::sync::Arc::new(
                vello_common::filter_effects::Filter::from_function(
                    vello_common::filter_effects::FilterFunction::Blur { radius: 1. },
                ),
            )),
            None,
        );
        s.pop_layer();
        assert_eq!(tracker.update(&s, 100, 100, bounds), DamageRegion::Full);
        assert_eq!(
            tracker.update(&scene(10.), 100, 100, bounds),
            DamageRegion::Full
        );
        assert_eq!(
            tracker.update(&scene(10.), 200, 100, bounds),
            DamageRegion::Full
        );
    }
}
