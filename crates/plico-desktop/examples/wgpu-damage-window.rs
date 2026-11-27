//! Paced real-window wgpu damage A/B probe. Compare PLICO_WGPU_DAMAGE=0/1.
//! Runs for 18 seconds at a 60 Hz deadline; measure after window/font warmup.
use plico::prelude::*;
use plico_desktop::{
    EffectPayload, EffectQueue, EffectRequest, HostView, Source, WindowCommand, WindowOptions,
};
use std::time::Duration;

#[derive(Default)]
struct Demo {
    start: Option<Duration>,
    close: bool,
    effects: EffectQueue,
    frames: u32,
}
impl HostView for Demo {
    fn take_effect(&mut self) -> Option<EffectRequest> {
        if std::mem::take(&mut self.close) {
            self.effects
                .push(EffectPayload::window(WindowCommand::Close));
        }
        self.effects.pop()
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        let now = cx.now();
        let elapsed = now.saturating_sub(*self.start.get_or_insert(now));
        self.frames += 1;
        if self.frames <= 2 || self.frames.is_multiple_of(60) {
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
            for row_index in 0..10u64 {
                cx.keyed(row_index, |cx| {
                    row(cx).gap(24) + |cx| {
                        for col in 0..4u64 {
                            cx.keyed(col, |cx| {
                                text!(cx, "Item {} · 中文 / 日本語", row_index * 4 + col);
                            });
                        }
                    };
                });
            }
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
    fn build_frame(
        &mut self,
        tcx: &mut plico_renderer::TextContext,
        w: u32,
        h: u32,
    ) -> Vec<plico_renderer::layout::PlacedNode> {
        self.source.build_frame(tcx, w, h)
    }
    fn render_frame(
        &mut self,
        scene: &mut plico_renderer::Scene,
        text: &mut plico_renderer::TextContext,
        size: (u32, u32),
        scale: f64,
    ) -> Result<(usize, f64, f64), plico_renderer::FrameSourceError> {
        let result = self.source.render_frame(scene, text, size, scale);
        self.next = std::time::Instant::now() + Duration::from_secs_f64(1. / 60.);
        result.map_err(|error| plico_renderer::FrameSourceError::new(error.to_string()))
    }
    fn base_color(&self) -> peniko::Color {
        self.source.base_color()
    }
    fn handle_event(&mut self, event: plico_renderer::UiEvent) -> plico_renderer::EventResponse {
        self.source.handle_event(event)
    }
    fn animation_deadline(&self) -> Option<std::time::Instant> {
        Some(self.next)
    }
    fn take_effect(&mut self) -> Option<EffectRequest> {
        self.source.take_effect()
    }
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    plico_desktop::run_source(
        Paced {
            source: Source::new(Demo::default()),
            next: std::time::Instant::now(),
        },
        WindowOptions::new()
            .title("Plico wgpu damage probe")
            .initial_inner_size(1280, 800),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_renderer::FrameSource;
    #[test]
    fn animation_probe_advances_and_requests_real_frames() {
        let mut ui = plico_desktop::testing::Tester::new(Demo::default(), 720, 640).unwrap();
        assert!(ui.has_animation());
        let before = ui.view().frames;
        ui.advance(Duration::from_millis(100)).unwrap();
        assert!(ui.view().frames > before);
        assert!(ui.has_animation());
        ui.advance(Duration::from_secs(19)).unwrap();
        assert!(ui.view().close);
    }
    #[test]
    fn retained_animation_probe_keeps_raf_across_native_frames() {
        let clock = plico::VirtualClock::default();
        let mut source = Source::with_clock(Demo::default(), clock.clone());
        let mut text = plico_renderer::TextContext::new();
        let mut scene = plico_renderer::Scene::new();
        for frame in 0..60 {
            clock.advance(Duration::from_millis(16)).unwrap();
            source.render_frame(&mut scene, &mut text, (720, 500), 1.75);
            assert!(source.has_anim(), "frame {frame}");
            assert!(source.view().frames >= frame + 1);
        }
    }
}
