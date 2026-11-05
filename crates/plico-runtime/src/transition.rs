//! Presentation-only theme transition: no intermediate palettes or text shaping.
use kurbo::{Affine, Circle};
use plico::Theme;
use plico_renderer::{PaintScene, Scene};
use std::time::Duration;
use web_time::Instant;

pub(crate) struct ThemeTransition {
    duration: Duration,
    enabled: bool,
    theme: Option<Theme>,
    last: Option<Scene>,
    from: Option<Scene>,
    from_theme: Option<Theme>,
    to: Option<Scene>,
    to_theme: Option<Theme>,
    start_progress: f32,
    end_progress: f32,
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
            from_theme: None,
            to: None,
            to_theme: None,
            start_progress: 0.,
            end_progress: 1.,
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
            self.clear_animation();
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
        self.clear_animation();
    }
    fn clear_animation(&mut self) {
        self.from = None;
        self.from_theme = None;
        self.to = None;
        self.to_theme = None;
        self.started = None;
    }
    fn progress(&self, now: Instant) -> f32 {
        let t = self.started.map_or(1., |start| {
            (now.saturating_duration_since(start).as_secs_f32() / self.duration.as_secs_f32())
                .clamp(0., 1.)
        });
        let eased = t * t * (3. - 2. * t);
        self.start_progress + (self.end_progress - self.start_progress) * eased
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
            self.clear_animation();
            self.size = Some(size);
        }
        let changed = self
            .theme
            .is_some_and(|previous| !previous.same_colors(theme));
        if changed && self.enabled && !self.duration.is_zero() {
            let progress = self.progress(now);
            if self.active() && self.from_theme.is_some_and(|t| t.same_colors(theme)) {
                // Reverse the same reveal geometry. Never use a composed frame
                // as an endpoint: rapid toggles would retain unbounded history.
                self.start_progress = progress;
                self.end_progress = 0.;
                self.started = Some(now);
                self.next_origin = None;
            } else if self.active() && self.to_theme.is_some_and(|t| t.same_colors(theme)) {
                self.start_progress = progress;
                self.end_progress = 1.;
                self.started = Some(now);
                self.next_origin = None;
            } else {
                self.origin = self
                    .next_origin
                    .take()
                    .unwrap_or((width as f64 / 2., height as f64 / 2.));
                // A third palette starts from the last uncomposed target. This
                // intentionally trades continuity for bounded scene ownership.
                self.from = self.last.take();
                self.from_theme = self.theme;
                self.to_theme = Some(theme);
                self.to = Some(scene.clone());
                self.start_progress = 0.;
                self.end_progress = 1.;
                self.started = self.from.as_ref().map(|_| now);
            }
        }
        self.theme = Some(theme);
        // Store clean endpoints, including live content updates in either palette.
        self.last = Some(scene.clone());
        if let Some(start) = self.started {
            if now.saturating_duration_since(start) >= self.duration {
                self.clear_animation();
            } else {
                if self.from_theme.is_some_and(|t| t.same_colors(theme)) {
                    self.from = Some(scene.clone());
                } else {
                    self.to = Some(scene.clone());
                }
                let progress = self.progress(now);
                if let (Some(from), Some(to)) = (&self.from, &self.to) {
                    *scene = from.clone();
                    let (x, y) = self.origin;
                    let radius =
                        x.max(width as f64 - x).hypot(y.max(height as f64 - y)) * progress as f64;
                    scene.push_clip_layer(
                        Affine::IDENTITY,
                        &Circle::new((x * scale, y * scale), radius * scale),
                    );
                    scene.append_scene(to.clone(), Affine::IDENTITY);
                    scene.pop_layer();
                }
            }
        }
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
        assert_eq!(
            reversed, middle,
            "reversal must preserve the presented reveal"
        );
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
    fn rapid_retargets_keep_only_two_clean_endpoints() {
        let now = Instant::now();
        let mut transition = ThemeTransition::default();
        let light = Theme::light();
        let dark = Theme::dark();
        let a = scene(Color::WHITE);
        let b = scene(Color::BLACK);
        transition.compose(&mut a.clone(), light, (2560, 1600, 2.), now);
        for step in 1..=100 {
            let (theme, target) = if step % 2 == 0 {
                (light, &a)
            } else {
                (dark, &b)
            };
            let mut frame = target.clone();
            transition.compose(
                &mut frame,
                theme,
                (2560, 1600, 2.),
                now + Duration::from_millis(step * 30),
            );
            assert_eq!(
                frame.commands.len(),
                a.commands.len() + b.commands.len() + 2
            );
            assert_eq!(transition.last.as_ref(), Some(target));
        }
        let mut final_frame = a.clone();
        transition.compose(
            &mut final_frame,
            light,
            (2560, 1600, 2.),
            now + Duration::from_millis(3201),
        );
        assert_eq!(final_frame, a);
        assert!(!transition.active());
        assert!(transition.from.is_none() && transition.to.is_none());
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
