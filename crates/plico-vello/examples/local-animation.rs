//! Real-shell CPU probe: a small animated marker among otherwise static UI.
//! Runs for 18 injected-clock seconds and closes itself. Compare launches with
//! PLICO_SKIA_DAMAGE=0/1; measure after the initial font/window warmup.
use plico_core::prelude::*;
use plico_vello::{HostAction, RenderBackend, Source, View, WindowCommand, WindowOptions};
use std::time::Duration;

#[derive(Default)]
struct Demo {
    start: Option<Duration>,
    close: bool,
    frames: u32,
}
impl View for Demo {
    fn take_host_action(&mut self) -> Option<HostAction> {
        std::mem::take(&mut self.close).then_some(HostAction::Window(WindowCommand::Close))
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        let now = cx.now();
        let elapsed = now.saturating_sub(*self.start.get_or_insert(now));
        self.frames += 1;
        if self.frames <= 2 {
            println!("frame={} elapsed={:.3}", self.frames, elapsed.as_secs_f64());
        }
        self.close = elapsed >= Duration::from_secs(18);
        if self.close {
            println!("frames={}", self.frames);
        } else {
            cx.request_animation_frame();
        }
        let seconds = elapsed.as_secs_f64();
        column(cx).width_percent(100.).height_percent(100.).padding(24).gap(24) + |cx| {
            text(cx, "Progress & spinners").font_size(24);
            for (i, label) in ["Download", "Indexing", "Upload"].into_iter().enumerate() {
                cx.keyed(i as u64, |cx| {
                    progress(cx, label, (0.5 + 0.45 * (seconds + i as f64).sin()) as f32);
                });
            }
            row(cx).gap(32) + |cx| {
                for i in 0..3u64 {
                    cx.keyed(i, |cx| spinner(cx, seconds + i as f64));
                }
            };
        };
    }
}
#[component]
#[rustfmt::skip]
fn spinner(cx: &mut UiContext, seconds: f64) {
    row(cx).width(36).height(36) + |cx| {
        for i in 0..8u64 {
            let angle = i as f64 * std::f64::consts::TAU / 8.;
            let alpha = (0.25 + 0.75 * (seconds * 5. - angle).sin().max(0.)) as f32;
            cx.keyed(i, |cx| {
                column(cx).absolute(
                    (15. + 13. * angle.cos()) as f32,
                    (15. + 13. * angle.sin()) as f32,
                ).width(6).height(6).radius(3.).background(Color::Accent).opacity(alpha);
            });
        }
    };
}
// The probe has no editable controls. Use the shell's deadline path to hold
// both A/B runs at 60 Hz, including Xvfb where FIFO may be effectively uncapped.
// This is benchmark pacing, not an application-wide frame limiter.
struct Paced {
    source: Source<Demo>,
    next: std::time::Instant,
}
impl plico_renderer::FrameSource for Paced {
    fn set_device_scale(&mut self, scale: f64) {
        self.source.set_device_scale(scale);
    }
    fn build_frame(
        &mut self,
        tcx: &mut plico_renderer::TextContext,
        w: u32,
        h: u32,
    ) -> Vec<plico_renderer::layout::PlacedNode> {
        self.source.build_frame(tcx, w, h)
    }
    fn render_retained_frame(
        &mut self,
        scene: &mut plico_renderer::Scene,
        text: &mut plico_renderer::TextContext,
        size: (u32, u32),
        scale: f64,
    ) -> Option<(usize, f64, f64)> {
        let result = self.source.render_retained_frame(scene, text, size, scale);
        self.next = std::time::Instant::now() + Duration::from_secs_f64(1. / 60.);
        result
    }
    fn base_color(&self) -> vello_common::peniko::Color {
        self.source.base_color()
    }
    fn handle_event(&mut self, event: plico_renderer::UiEvent) -> plico_renderer::EventResponse {
        self.source.handle_event(event)
    }
    fn animation_deadline(&self) -> Option<std::time::Instant> {
        Some(self.next)
    }
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.source.take_host_action()
    }
}
fn main() -> Result<(), plico_vello::DesktopError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    plico_vello::run_source_with_options(
        Paced {
            source: Source::new(Demo::default()),
            next: std::time::Instant::now(),
        },
        WindowOptions::new()
            .title("Plico local animation probe")
            .initial_inner_size(720, 500)
            .renderer(RenderBackend::Skia),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_renderer::FrameSource;
    #[test]
    fn animation_probe_advances_and_requests_real_frames() {
        let mut ui = plico_vello::testing::Driver::new(Demo::default(), 720, 640).unwrap();
        assert!(ui.source.has_anim());
        let before = ui.source.view.frames;
        ui.advance(Duration::from_millis(100)).unwrap();
        assert!(ui.source.view.frames > before);
        assert!(ui.source.has_anim());
        ui.advance(Duration::from_secs(19)).unwrap();
        assert!(ui.source.view.close);
    }
    #[test]
    fn retained_animation_probe_keeps_raf_across_native_frames() {
        let clock = plico_core::VirtualClock::default();
        let mut source = Source::with_clock(Demo::default(), clock.clone());
        let mut text = plico_renderer::TextContext::new();
        let mut scene = plico_renderer::Scene::new();
        for frame in 0..60 {
            clock.advance(Duration::from_millis(16)).unwrap();
            source.render_retained_frame(&mut scene, &mut text, (720, 500), 1.75);
            assert!(source.last_error.is_none());
            assert!(source.has_anim(), "frame {frame}");
            assert!(source.view.frames >= frame + 1);
        }
    }
}
