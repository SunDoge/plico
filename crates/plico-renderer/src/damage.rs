//! Optional redraw information, independent of layout and window APIs.
use kurbo::Rect;

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
    fn sparse_rects_are_bounded_disjoint_and_cover_all_original_damage() {
        let mut regions = DamageRects {
            limit: 8,
            ..Default::default()
        };
        let originals: Vec<_> = (0..20)
            .map(|i| {
                Rect::new(
                    (i % 5) as f64 * 20.,
                    (i / 5) as f64 * 20.,
                    (i % 5) as f64 * 20. + 3.,
                    (i / 5) as f64 * 20. + 3.,
                )
            })
            .collect();
        for rect in &originals {
            regions.add(DamageRegion::Rect(*rect));
        }
        assert!(regions.rects().len() <= 8);
        for original in originals {
            assert!(regions.rects().iter().any(|r| r.union(original) == *r));
        }
        for (i, a) in regions.rects().iter().enumerate() {
            for b in &regions.rects()[i + 1..] {
                assert!(a.intersect(*b).area() == 0.);
            }
        }
        assert!(regions.coverage(100, 100) <= regions.bounding_region().coverage(100, 100));
        regions.add(DamageRegion::Full);
        assert!(regions.is_full());
        assert_eq!(regions.coverage(100, 100), 1.);
    }
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

/// A bounded, non-overlapping set of conservative physical-pixel damage rects.
/// Full damage has no finite rectangle list; use [`Self::is_full`] first.
#[derive(Debug)]
pub struct DamageRects {
    full: bool,
    rects: Vec<Rect>,
    limit: usize,
}
impl Default for DamageRects {
    fn default() -> Self {
        Self {
            full: false,
            rects: Vec::new(),
            limit: 1,
        }
    }
}
impl DamageRects {
    fn clear(&mut self) {
        self.full = false;
        self.rects.clear();
    }
    fn add(&mut self, region: DamageRegion) {
        if self.full {
            return;
        }
        let mut rect = match region {
            DamageRegion::Empty => return,
            DamageRegion::Full => {
                self.full = true;
                self.rects.clear();
                return;
            }
            DamageRegion::Rect(rect) => rect,
        };
        if self.limit == 1 && !self.rects.is_empty() {
            self.rects[0] = self.rects[0].union(rect);
            return;
        }
        let mut i = 0;
        while i < self.rects.len() {
            let old = self.rects[i];
            if old.x0 <= rect.x1 && rect.x0 <= old.x1 && old.y0 <= rect.y1 && rect.y0 <= old.y1 {
                rect = rect.union(self.rects.swap_remove(i));
                i = 0;
            } else {
                i += 1;
            }
        }
        self.rects.push(rect);
        if self.rects.len() > self.limit {
            let mut best = (0, 1, f64::INFINITY);
            for i in 0..self.rects.len() {
                for j in i + 1..self.rects.len() {
                    let cost = self.rects[i].union(self.rects[j]).area()
                        - self.rects[i].area()
                        - self.rects[j].area();
                    if cost < best.2 {
                        best = (i, j, cost);
                    }
                }
            }
            let b = self.rects.swap_remove(best.1);
            let a = self.rects.swap_remove(best.0);
            self.add(DamageRegion::Rect(a.union(b)));
        }
    }
    /// Whether the complete target must be redrawn.
    pub fn is_full(&self) -> bool {
        self.full
    }
    /// Disjoint rectangles, rounded/clamped by the tracker to its target.
    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }
    /// Conservative bounding damage, retaining the original single-rect API.
    pub fn bounding_region(&self) -> DamageRegion {
        if self.full {
            DamageRegion::Full
        } else {
            self.rects
                .iter()
                .copied()
                .map(DamageRegion::Rect)
                .fold(DamageRegion::Empty, DamageRegion::union)
        }
    }
    /// Target coverage after clipping. Rectangles are disjoint, so no overlap
    /// is double counted. This differs from the bounding rectangle's coverage.
    pub fn coverage(&self, width: u32, height: u32) -> f64 {
        if width == 0 || height == 0 {
            return 0.;
        }
        if self.full {
            return 1.;
        }
        let viewport = Rect::new(0., 0., width as f64, height as f64);
        self.rects
            .iter()
            .map(|r| r.intersect(viewport).area())
            .sum::<f64>()
            / viewport.area()
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
    regions: DamageRects,
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
    /// Opt into up to eight disjoint rectangles; otherwise accumulate one bounding rect.
    pub fn set_multi_rect_damage(&mut self, enabled: bool) {
        self.regions.limit = if enabled { 8 } else { 1 };
    }
    /// Damage rectangles from the most recent update.
    pub fn damage_rects(&self) -> &DamageRects {
        &self.regions
    }
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
        self.regions.clear();
        self.regions.add(DamageRegion::Full);
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
        use kurbo::Shape;
        use peniko::BlendMode;
        self.regions.clear();
        if self.previous.is_none() || self.size != Some((width, height)) || self.nonlocal {
            self.regions.add(DamageRegion::Full);
        }
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
                        self.regions.add(DamageRegion::Full);
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
                        None => self.regions.add(DamageRegion::Full),
                    }
                    bounds
                }
                PaintCommand::Fragment { scene, .. } if scene.has_nonlocal_effects() => {
                    self.nonlocal = true;
                    self.regions.add(DamageRegion::Full);
                    DamageRegion::Full
                }
                PaintCommand::Shader { .. } => {
                    self.nonlocal = true;
                    self.regions.add(DamageRegion::Full);
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
                self.regions.add(bounds);
                if let Some(old) = self.bounds.get(index) {
                    self.regions.add(*old);
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
            self.regions.add(*removed);
        }
        self.bounds.truncate(scene.commands.len());
        self.contexts.truncate(scene.commands.len());
        if !clips.is_empty() {
            self.regions.add(DamageRegion::Full);
        }
        self.expand_to_whole_command_bounds(&scene.commands);
        self.previous = Some(scene.clone());
        self.size = Some((width, height));
        self.regions.bounding_region().for_surface(width, height)
    }

    /// Do not let the outer damage clip cut through text or stroked AA geometry.
    /// Retained fragments are atomic here: expand to their complete bounds so
    /// nested text/strokes are not cut by the outer damage clip. Plain fills
    /// keep their original damage; unknown bounds may reach Full.
    fn expand_to_whole_command_bounds(&mut self, commands: &[crate::PaintCommand]) {
        use crate::PaintCommand;
        let mut index = 0;
        while index < self.regions.rects.len() {
            let damage = self.regions.rects[index];
            let mut expanded = false;
            for (command, bounds) in commands.iter().zip(&self.bounds) {
                if !matches!(
                    command,
                    PaintCommand::Paragraph { .. }
                        | PaintCommand::Fragment { .. }
                        | PaintCommand::Stroke { .. }
                        | PaintCommand::GlyphRun { .. }
                ) {
                    continue;
                }
                match *bounds {
                    DamageRegion::Full => {
                        self.regions.add(DamageRegion::Full);
                        return;
                    }
                    DamageRegion::Rect(bounds)
                        if damage.intersect(bounds).area() > 0.
                            && damage.union(bounds) != damage =>
                    {
                        self.regions.add(DamageRegion::Rect(bounds));
                        expanded = true;
                        break;
                    }
                    DamageRegion::Empty | DamageRegion::Rect(_) => {}
                }
            }
            if expanded {
                index = 0;
            } else {
                index += 1;
            }
        }
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
    use kurbo::{Affine, Shape};
    use peniko::{Color, Fill};
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
            peniko::BlendMode::default(),
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
    #[test]
    fn nested_nonlocal_effects_invalidate_unchanged_frames_and_removal() {
        let mut filtered = Scene::new();
        filtered.push_layer(
            peniko::BlendMode::default(),
            1.,
            Affine::IDENTITY,
            &Rect::new(0., 0., 10., 10.),
            Some(std::sync::Arc::new(
                vello_common::filter_effects::Filter::from_function(
                    vello_common::filter_effects::FilterFunction::Blur { radius: 1. },
                ),
            )),
            None,
        );
        filtered.pop_layer();
        let mut inner = Scene::new();
        inner.append_cached_scene(filtered, Affine::IDENTITY);
        let mut outer = Scene::new();
        outer.append_cached_scene(inner, Affine::IDENTITY);
        assert!(outer.has_nonlocal_effects());
        let mut tracker = SceneDamageTracker::default();
        for _ in 0..3 {
            assert_eq!(
                tracker.update(&outer, 100, 100, |_| Some(Rect::new(0., 0., 10., 10.))),
                DamageRegion::Full
            );
        }
        let empty = Scene::new();
        assert!(!empty.has_nonlocal_effects());
        assert_eq!(tracker.update(&empty, 100, 100, bounds), DamageRegion::Full);
        assert_eq!(
            tracker.update(&empty, 100, 100, bounds),
            DamageRegion::Empty
        );
    }

    #[test]
    fn aa_commands_expand_partial_intersections_but_plain_fills_do_not() {
        let paragraph = PaintCommand::Paragraph {
            request: std::sync::Arc::new(
                crate::paragraph::ParagraphRequest::from_paint(
                    &crate::style::Paint {
                        text: Some("AA text".into()),
                        ..Default::default()
                    },
                    None,
                )
                .unwrap(),
            ),
            transform: Affine::IDENTITY,
        };
        let mut stroked = Scene::new();
        stroked.stroke(
            &kurbo::Stroke::new(2.),
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &Rect::new(0., 0., 60., 60.),
        );
        let stroke = stroked.commands[0].clone();
        for command in [paragraph, stroke] {
            let mut tracker = SceneDamageTracker {
                bounds: vec![DamageRegion::Rect(Rect::new(0., 0., 60., 60.))],
                ..Default::default()
            };
            tracker
                .regions
                .add(DamageRegion::Rect(Rect::new(10., 10., 20., 20.)));
            tracker.expand_to_whole_command_bounds(&[command]);
            assert_eq!(
                tracker.regions.bounding_region(),
                DamageRegion::Rect(Rect::new(0., 0., 60., 60.))
            );
        }

        let fill = scene(0.).commands[0].clone();
        let mut tracker = SceneDamageTracker {
            bounds: vec![DamageRegion::Rect(Rect::new(0., 0., 60., 60.))],
            ..Default::default()
        };
        tracker
            .regions
            .add(DamageRegion::Rect(Rect::new(10., 10., 20., 20.)));
        tracker.expand_to_whole_command_bounds(&[fill]);
        assert_eq!(
            tracker.regions.bounding_region(),
            DamageRegion::Rect(Rect::new(10., 10., 20., 20.))
        );
    }

    #[test]
    fn display_damage_is_not_the_acquired_buffers_damage() {
        let (a, b) = (scene(10.), scene(40.));
        let mut buffer_a = SceneDamageTracker::default();
        let mut buffer_b = SceneDamageTracker::default();
        let mut display = SceneDamageTracker::default();
        buffer_a.update(&a, 100, 100, bounds);
        display.update(&a, 100, 100, bounds);
        buffer_b.update(&b, 100, 100, bounds);
        display.update(&b, 100, 100, bounds);
        assert_eq!(buffer_a.update(&a, 100, 100, bounds), DamageRegion::Empty);
        let DamageRegion::Rect(r) = display.update(&a, 100, 100, bounds) else {
            panic!()
        };
        assert!(r.x0 <= 10. && r.x1 >= 50.);
    }
}
