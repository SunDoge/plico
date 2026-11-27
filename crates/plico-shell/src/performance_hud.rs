//! Renderer-native performance diagnostics drawn outside the application tree.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use kurbo::{Affine, Rect};
use peniko::{Color, Fill};

use plico_renderer::source::{FrameSource, FrameSourceError, WindowLevel, WindowOptions};
use plico_renderer::{FrameStats, PaintScene, Scene, TextContext};

const PANEL_WIDTH: f64 = 320.0;
const PANEL_HEIGHT: f64 = 212.0;
const PANEL_MARGIN: f64 = 12.0;
const PRIMARY_TEXT: [u8; 4] = [15, 23, 42, 255];
const SECONDARY_TEXT: [u8; 4] = [49, 60, 79, 255];
const REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const IDLE_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) struct HudMetrics {
    panel: PerformanceHud,
    stats: FrameStats,
    dirty: bool,
    next_refresh: Option<Instant>,
}

impl HudMetrics {
    pub(crate) fn from_environment() -> Option<Self> {
        std::env::var("PLICO_PERFORMANCE_HUD")
            .is_ok_and(|value| parse_enabled(&value))
            .then(Self::new)
    }

    fn new() -> Self {
        Self {
            panel: PerformanceHud::new(true, false),
            stats: FrameStats::default(),
            dirty: false,
            next_refresh: None,
        }
    }

    pub(crate) fn observe(&mut self, now: Instant, stats: FrameStats, presented: bool) {
        if presented {
            self.panel.begin_frame(now);
        }
        self.stats = stats;
        self.dirty = true;
    }

    /// Return the next sample deadline, or consume one ready update.
    pub(crate) fn poll(&mut self, now: Instant) -> Result<bool, Instant> {
        let idle_at = self
            .panel
            .last_frame
            .filter(|_| self.panel.fps != 0.)
            .map(|time| time + IDLE_INTERVAL);
        if idle_at.is_some_and(|deadline| deadline <= now) {
            self.panel.fps = 0.;
            self.dirty = true;
        }
        if !self.dirty {
            return idle_at.map_or(Ok(false), Err);
        }
        if let Some(deadline) = self.next_refresh.filter(|deadline| *deadline > now) {
            return Err(deadline);
        }
        self.dirty = false;
        self.next_refresh = Some(now + REFRESH_INTERVAL);
        Ok(true)
    }
}

pub(crate) struct HudSource(pub(crate) Rc<RefCell<HudMetrics>>);

impl HudSource {
    pub(crate) fn window_options() -> WindowOptions {
        WindowOptions::new()
            .title("Plico Performance HUD")
            .initial_inner_size(
                (PANEL_WIDTH + PANEL_MARGIN * 2.) as u32,
                (PANEL_HEIGHT + PANEL_MARGIN * 2.) as u32,
            )
            .resizable(false)
            .window_level(WindowLevel::AlwaysOnTop)
            .activate_on_show(false)
    }
}

impl FrameSource for HudSource {
    fn build_frame(
        &mut self,
        _: &mut TextContext,
        _: u32,
        _: u32,
    ) -> Vec<plico_renderer::layout::PlacedNode> {
        Vec::new()
    }

    fn base_color(&self) -> Color {
        Color::from_rgb8(232, 235, 240)
    }

    fn render_frame(
        &mut self,
        scene: &mut Scene,
        text: &mut TextContext,
        (width, height): (u32, u32),
        scale: f64,
    ) -> Result<(usize, f64, f64), FrameSourceError> {
        let start = Instant::now();
        *scene = Scene::new();
        text.set_native_paragraph_scale(scale);
        let sample = self.0.borrow();
        sample
            .panel
            .paint(scene, text, sample.stats, [width, height], scale);
        Ok((0, 0., start.elapsed().as_secs_f64() * 1000.))
    }
}

pub(crate) struct PerformanceHud {
    enabled: bool,
    translucent: bool,
    last_frame: Option<Instant>,
    fps: f64,
}

impl PerformanceHud {
    pub(crate) fn inline_from_environment() -> Option<Self> {
        std::env::var("PLICO_PERFORMANCE_HUD")
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case("inline"))
            .then(|| Self::new(true, true))
    }

    fn new(enabled: bool, translucent: bool) -> Self {
        Self {
            enabled,
            translucent,
            last_frame: None,
            fps: 0.0,
        }
    }

    pub(crate) fn begin_frame(&mut self, now: Instant) {
        if !self.enabled {
            return;
        }
        if let Some(previous) = self.last_frame {
            let seconds = now.duration_since(previous).as_secs_f64();
            if seconds > IDLE_INTERVAL.as_secs_f64() {
                // A long gap on the demand-driven window starts a new sample.
                self.fps = 0.0;
            } else if seconds > 0.0 {
                let sample = (1.0 / seconds).min(999.0);
                self.fps = if self.fps == 0.0 {
                    sample
                } else {
                    self.fps * 0.85 + sample * 0.15
                };
            }
        }
        self.last_frame = Some(now);
    }

    pub(crate) fn paint(
        &self,
        scene: &mut Scene,
        text: &mut TextContext,
        stats: FrameStats,
        viewport: [u32; 2],
        scale: f64,
    ) {
        if !self.enabled
            || f64::from(viewport[0]) < PANEL_WIDTH + PANEL_MARGIN * 2.0
            || f64::from(viewport[1]) < PANEL_HEIGHT + PANEL_MARGIN * 2.0
        {
            return;
        }
        let left = f64::from(viewport[0]) - PANEL_WIDTH - PANEL_MARGIN;
        let top = PANEL_MARGIN;
        let panel = Rect::new(left, top, left + PANEL_WIDTH, top + PANEL_HEIGHT);
        let device = Affine::scale(scale);
        scene.draw_box_shadow(device, panel, Color::from_rgba8(15, 23, 42, 22), 8.0, 8.0);
        scene.fill(
            Fill::NonZero,
            device,
            Color::from_rgba8(248, 250, 252, if self.translucent { 174 } else { 255 }),
            None,
            &panel.to_rounded_rect(9.0),
        );
        scene.stroke(
            &kurbo::Stroke::new(1.0),
            device,
            Color::from_rgba8(203, 213, 225, 115),
            None,
            &panel.to_rounded_rect(9.0),
        );

        let total = stats.build_frame_ms + stats.scene_ms + stats.present_ms;
        let health = health_color(total);
        scene.fill(
            Fill::NonZero,
            device,
            health,
            None,
            &Rect::new(left + 14.0, top + 16.0, left + 18.0, top + 35.0).to_rounded_rect(2.0),
        );

        self.text(
            scene,
            text,
            "Plico HUD",
            [left + 26.0, top + 12.0],
            13.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );
        let cadence = if self.fps == 0.0 {
            "idle".to_owned()
        } else {
            format!("{:>3.0} fps", self.fps)
        };
        self.text(
            scene,
            text,
            &cadence,
            [left + 268.0, top + 12.0],
            12.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );

        self.text(
            scene,
            text,
            "Frame",
            [left + 14.0, top + 40.0],
            12.5,
            400.0,
            SECONDARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            &format!("{total:.2} ms"),
            [left + 60.0, top + 40.0],
            12.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            "Nodes",
            [left + 188.0, top + 40.0],
            12.5,
            400.0,
            SECONDARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            &stats.node_count.to_string(),
            [left + 234.0, top + 40.0],
            12.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );

        for (label, value, x) in [
            ("Declare", stats.sample_declaration_ms, 14.0),
            ("Backend", stats.sample_backend_ms, 116.0),
            ("Scene", stats.sample_scene_ms, 230.0),
        ] {
            self.text(
                scene,
                text,
                label,
                [left + x, top + 64.0],
                12.5,
                400.0,
                SECONDARY_TEXT,
                scale,
            );
            let value_x = x + match label {
                "Declare" => 48.0,
                "Backend" => 51.0,
                _ => 38.0,
            };
            self.text(
                scene,
                text,
                &format!("{value:.2}"),
                [left + value_x, top + 64.0],
                12.5,
                600.0,
                PRIMARY_TEXT,
                scale,
            );
        }

        self.text(
            scene,
            text,
            "Present",
            [left + 14.0, top + 88.0],
            12.5,
            400.0,
            SECONDARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            &format!("{:.2} ms", stats.present_ms),
            [left + 66.0, top + 88.0],
            12.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            "Viewport",
            [left + 188.0, top + 88.0],
            12.5,
            400.0,
            SECONDARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            &format!("{}×{}", stats.viewport_w, stats.viewport_h),
            [left + 244.0, top + 88.0],
            12.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            "Damage",
            [left + 14.0, top + 136.0],
            12.5,
            400.0,
            SECONDARY_TEXT,
            scale,
        );
        let coverage = stats
            .damage_coverage
            .map_or_else(|| "n/a".to_owned(), |value| format!("{:.1}%", value * 100.));
        self.text(
            scene,
            text,
            &coverage,
            [left + 66.0, top + 136.0],
            12.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            "GPU",
            [left + 188.0, top + 136.0],
            12.5,
            400.0,
            SECONDARY_TEXT,
            scale,
        );
        self.text(
            scene,
            text,
            &stats
                .gpu_ms
                .map_or_else(|| "n/a".to_owned(), |value| format!("{value:.2} ms")),
            [left + 224.0, top + 136.0],
            12.5,
            600.0,
            PRIMARY_TEXT,
            scale,
        );
        for (label, value, label_x, value_x) in [
            ("Prepare", stats.prepare_ms, 14.0, 68.0),
            ("Upload", stats.upload_ms, 118.0, 170.0),
            ("Encode", stats.encode_ms, 218.0, 270.0),
        ] {
            self.text(
                scene,
                text,
                label,
                [left + label_x, top + 160.0],
                12.5,
                400.0,
                SECONDARY_TEXT,
                scale,
            );
            self.text(
                scene,
                text,
                &format!("{value:.2}"),
                [left + value_x, top + 160.0],
                12.5,
                600.0,
                PRIMARY_TEXT,
                scale,
            );
        }
        for (label, value, label_x, value_x) in [
            ("Draws", stats.draw_calls.to_string(), 14.0, 60.0),
            ("Instances", stats.instances.to_string(), 118.0, 182.0),
            (
                "Up",
                format!("{}K", stats.upload_bytes / 1024),
                238.0,
                262.0,
            ),
        ] {
            self.text(
                scene,
                text,
                label,
                [left + label_x, top + 184.0],
                12.5,
                400.0,
                SECONDARY_TEXT,
                scale,
            );
            self.text(
                scene,
                text,
                &value,
                [left + value_x, top + 184.0],
                12.5,
                600.0,
                PRIMARY_TEXT,
                scale,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        scene: &mut Scene,
        text: &mut TextContext,
        value: &str,
        origin: [f64; 2],
        size: f32,
        weight: f32,
        color: [u8; 4],
        scale: f64,
    ) {
        let layout = plico_renderer::text::layout_text_styled(
            text,
            Arc::from(value),
            size,
            weight,
            false,
            None,
            plico_renderer::style::TextAlign::Start,
            color,
            Arc::from([]),
            None,
            None,
        );
        let device = Affine::scale(scale);
        plico_renderer::scene::draw_text_layout_into(
            scene,
            text,
            &layout,
            Affine::IDENTITY,
            device * Affine::translate((origin[0], origin[1])),
            scale,
        );
    }
}

fn health_color(milliseconds: f64) -> Color {
    if milliseconds <= 0.0 || !milliseconds.is_finite() {
        Color::from_rgb8(148, 163, 184)
    } else if milliseconds < 8.0 {
        Color::from_rgb8(21, 128, 61)
    } else if milliseconds < 16.7 {
        Color::from_rgb8(180, 83, 9)
    } else {
        Color::from_rgb8(185, 28, 28)
    }
}

fn parse_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    #[cfg(all(target_os = "macos", feature = "wgpu"))]
    fn hud_text_context(scale: f64) -> TextContext {
        let mut text = TextContext::new();
        // Match Shell's native text setup before recording paint commands.
        // The standalone image renderer cannot consume raw Parley glyph IDs
        // on the CoreText path. The engine owns its shared cache after this
        // temporary renderer is dropped.
        let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
        renderer.configure_text(&mut text, scale).unwrap();
        text
    }
    #[cfg(not(all(target_os = "macos", feature = "wgpu")))]
    fn hud_text_context(_scale: f64) -> TextContext {
        TextContext::new()
    }

    #[test]
    fn hud_flag_and_health_thresholds_are_explicit() {
        for value in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(parse_enabled(value));
        }
        for value in ["", "0", "false", "enabled"] {
            assert!(!parse_enabled(value));
        }
        assert_eq!(health_color(4.0), Color::from_rgb8(21, 128, 61));
        assert_eq!(health_color(12.0), Color::from_rgb8(180, 83, 9));
        assert_eq!(health_color(20.0), Color::from_rgb8(185, 28, 28));

        let start = Instant::now();
        let mut hud = PerformanceHud::new(true, true);
        hud.begin_frame(start);
        hud.begin_frame(start + std::time::Duration::from_millis(16));
        assert!(hud.fps > 60.0);
        hud.begin_frame(start + std::time::Duration::from_secs(1));
        assert_eq!(hud.fps, 0.0);
    }

    #[test]
    fn hud_refresh_coalesces_samples_and_goes_idle_without_polling_the_owner() {
        let start = Instant::now();
        let mut metrics = HudMetrics::new();
        assert_eq!(metrics.poll(start), Ok(false));
        metrics.observe(start, FrameStats::default(), true);
        assert_eq!(metrics.poll(start), Ok(true));
        let next = start + Duration::from_millis(16);
        metrics.observe(
            next,
            FrameStats {
                damage_coverage: Some(0.05),
                ..FrameStats::default()
            },
            true,
        );
        metrics.observe(
            next + Duration::from_millis(1),
            FrameStats {
                damage_coverage: Some(0.08),
                ..FrameStats::default()
            },
            false,
        );
        let refresh = start + REFRESH_INTERVAL;
        assert_eq!(metrics.poll(next), Err(refresh));
        assert_eq!(metrics.poll(refresh), Ok(true));
        assert_eq!(metrics.stats.damage_coverage, Some(0.08));
        let idle = next + IDLE_INTERVAL;
        assert_eq!(metrics.poll(refresh), Err(idle));
        assert_eq!(metrics.poll(idle), Ok(true));
        assert_eq!(metrics.panel.fps, 0.);
        assert_eq!(metrics.poll(idle), Ok(false));
    }

    #[test]
    fn hud_paints_outside_the_application_scene_when_enabled() {
        let mut scene = Scene::new();
        let mut text = hud_text_context(1.0);
        PerformanceHud::new(false, true).paint(
            &mut scene,
            &mut text,
            FrameStats::default(),
            [700, 260],
            1.0,
        );
        assert!(scene.commands.is_empty());

        PerformanceHud::new(true, true).paint(
            &mut scene,
            &mut text,
            FrameStats {
                build_frame_ms: 2.0,
                scene_ms: 1.0,
                present_ms: 2.0,
                node_count: 42,
                viewport_w: 640,
                viewport_h: 480,
                ..FrameStats::default()
            },
            [700, 260],
            1.0,
        );
        assert!(scene.commands.len() >= 20);
        let image = crate::renderer::render_to_image(&scene, 700, 260, Color::BLACK).unwrap();
        let panel = image.get_pixel(500, 52).0;
        assert!(
            (140..=220).contains(&panel[0]),
            "expected translucent HUD surface: {panel:?}"
        );
        let light_image = crate::renderer::render_to_image(&scene, 700, 260, Color::WHITE).unwrap();
        let light_panel = light_image.get_pixel(500, 52).0;
        assert!(
            light_panel[0].saturating_sub(panel[0]) > 25,
            "the application surface must show through the HUD"
        );
        let darkest_body_pixel = image
            .view(380, 48, 300, 60)
            .pixels()
            .map(|(_, _, pixel)| {
                u16::from(pixel.0[0]) + u16::from(pixel.0[1]) + u16::from(pixel.0[2])
            })
            .min()
            .unwrap();
        assert!(
            darkest_body_pixel < 350,
            "expected high-contrast rasterized HUD text, got {darkest_body_pixel}"
        );

        let mut tiny_scene = Scene::new();
        PerformanceHud::new(true, true).paint(
            &mut tiny_scene,
            &mut text,
            FrameStats::default(),
            [240, 120],
            1.0,
        );
        assert!(tiny_scene.commands.is_empty());
    }

    #[test]
    fn hud_text_uses_the_same_logical_position_at_one_and_two_x() {
        for scale in [1_u32, 2] {
            let mut scene = Scene::new();
            let mut text = hud_text_context(f64::from(scale));
            PerformanceHud::new(true, true).paint(
                &mut scene,
                &mut text,
                FrameStats::default(),
                [700, 260],
                f64::from(scale),
            );
            let image =
                crate::renderer::render_to_image(&scene, 700 * scale, 260 * scale, Color::BLACK)
                    .unwrap();
            let header = image.view(390 * scale, 18 * scale, 110 * scale, 24 * scale);
            let dark_pixels = header
                .pixels()
                .filter(|(_, _, pixel)| {
                    u16::from(pixel.0[0]) + u16::from(pixel.0[1]) + u16::from(pixel.0[2]) < 350
                })
                .count();
            assert!(
                dark_pixels > 10 * scale as usize,
                "HUD heading must remain inside its card at {scale}x; found {dark_pixels} dark pixels",
            );
        }
    }
}
