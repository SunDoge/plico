//! Presentation-only theme transition: no intermediate palettes or text shaping.
use kurbo::{Affine, Circle};
use plico_core::Theme;
use plico_renderer::{PaintScene, Scene};
use std::time::Duration;
use web_time::Instant;

pub(crate) struct ThemeTransition {
    duration: Duration,
    enabled: bool,
    theme: Option<Theme>,
    last: Option<Scene>,
    from: Option<Scene>,
    started: Option<Instant>,
    size: Option<(u32, u32, u64)>,
    next_origin: Option<(f64, f64)>,
    origin: (f64, f64),
}
impl Default for ThemeTransition {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(200),
            enabled: true,
            theme: None,
            last: None,
            from: None,
            started: None,
            size: None,
            next_origin: None,
            origin: (0., 0.),
        }
    }
}
impl ThemeTransition {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.from = None;
            self.started = None;
        }
    }
    pub fn set_origin(&mut self, x: f64, y: f64) {
        self.next_origin = Some((x, y));
    }
    pub fn active(&self) -> bool {
        self.started.is_some()
    }
    pub fn set_duration(&mut self, duration: Duration) {
        self.duration = duration;
        self.from = None;
        self.started = None;
    }
    pub fn compose(
        &mut self,
        scene: &mut Scene,
        theme: Theme,
        size: (u32, u32, f64),
        now: Instant,
    ) {
        let (width, height, scale) = size;
        let size = (width, height, scale.to_bits());
        if self.size != Some(size) {
            self.last = None;
            self.from = None;
            self.started = None;
            self.size = Some(size);
        }
        if self
            .theme
            .is_some_and(|previous| !previous.same_colors(theme))
            && self.enabled
            && !self.duration.is_zero()
        {
            // Retarget from the last presented scene, including a rapid reversal.
            self.origin = self
                .next_origin
                .take()
                .unwrap_or((width as f64 / 2., height as f64 / 2.));
            self.from = self.last.take();
            self.started = self.from.as_ref().map(|_| now);
        }
        self.theme = Some(theme);
        if let (Some(start), Some(from)) = (self.started, &self.from) {
            let t = (now.saturating_duration_since(start).as_secs_f32()
                / self.duration.as_secs_f32())
            .clamp(0., 1.);
            if t < 1. {
                let alpha = t * t * (3. - 2. * t);
                let target = scene.clone();
                *scene = from.clone();
                let (x, y) = self.origin;
                let radius = x.max(width as f64 - x).hypot(y.max(height as f64 - y)) * alpha as f64;
                scene.push_clip_layer(
                    Affine::IDENTITY,
                    &Circle::new((x * scale, y * scale), radius * scale),
                );
                scene.append_scene(target, Affine::IDENTITY);
                scene.pop_layer();
            } else {
                self.from = None;
                self.started = None;
            }
        }
        self.last = Some(scene.clone());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;
    use peniko::{Color, Fill};
    fn scene(color: Color) -> Scene {
        let mut s = Scene::new();
        s.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &Rect::new(0., 0., 100., 100.),
        );
        s
    }
    #[test]
    fn transition_retargets_and_finishes_with_simulated_time() {
        let now = Instant::now();
        let light = Theme::default();
        let dark = Theme {
            surface: 0x111111,
            ..light
        };
        let a = scene(Color::WHITE);
        let b = scene(Color::BLACK);
        let mut transition = ThemeTransition::default();
        transition.compose(&mut a.clone(), light, (100, 100, 1.), now);
        assert!(!transition.active());
        transition.compose(&mut b.clone(), dark, (100, 100, 1.), now);
        assert!(transition.active());
        let mut middle = b.clone();
        transition.compose(
            &mut middle,
            dark,
            (100, 100, 1.),
            now + Duration::from_millis(100),
        );
        assert_ne!(middle, b);
        let mut reversed = a.clone();
        transition.compose(
            &mut reversed,
            light,
            (100, 100, 1.),
            now + Duration::from_millis(100),
        );
        assert!(transition.active());
        let mut end = a.clone();
        transition.compose(
            &mut end,
            light,
            (100, 100, 1.),
            now + Duration::from_millis(301),
        );
        assert!(!transition.active());
        assert_eq!(end, a);
    }
    #[test]
    fn resize_and_reduced_motion_do_not_leave_animation_running() {
        let now = Instant::now();
        let a = Theme::default();
        let b = Theme { surface: 0, ..a };
        let mut t = ThemeTransition::default();
        let mut s = Scene::new();
        t.compose(&mut s, a, (100, 100, 1.), now);
        t.compose(&mut s, b, (100, 100, 1.), now);
        assert!(t.active());
        t.compose(&mut s, b, (200, 100, 1.), now);
        assert!(!t.active());
        t.set_duration(Duration::ZERO);
        t.compose(&mut s, a, (200, 100, 1.), now);
        assert!(!t.active());
    }
}
