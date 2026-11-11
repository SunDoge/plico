//! Paced wgpu software-adapter probe. Pin Lavapipe's ICD externally; this probe
//! never changes driver selection. All animation uses the injected clock.
use plico::prelude::*;
use plico_desktop::{HostView, RenderBackend, Source, WindowOptions};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::{Duration, Instant};
static EFFECT: ShaderAsset = ShaderAsset::new(
    "fn plico_effect(uv: vec2<f32>) -> vec4<f32> { let wave = 0.5 + 0.5 * sin(uv.x * 12.0 - plico.resolution_time_scale.z * 2.0); return vec4<f32>(0.1, wave * 0.6 + 0.2, 0.9, 1.0); }",
);

#[derive(usage_rs::Cli)]
struct Options {
    /// idle, geometry (no text), animation (static text), scroll, both, or shader.
    #[usage(long, default = "animation")]
    workload: String,
    #[usage(long, default = "10")]
    seconds: u64,
}
struct Demo {
    workload: String,
    labels: Vec<String>,
    start: Option<Duration>,
    duration: Duration,
    close: bool,
}
impl HostView for Demo {
    fn take_host_action(&mut self) -> Option<plico_desktop::HostAction> {
        std::mem::take(&mut self.close).then_some(plico_desktop::HostAction::Window(
            plico_desktop::WindowCommand::Close,
        ))
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        let now = cx.now();
        let elapsed = now.saturating_sub(*self.start.get_or_insert(now));
        let active = self.workload != "idle";
        self.close = active && elapsed >= self.duration;
        if active && !self.close {
            cx.request_animation_frame();
        }
        let t = elapsed.as_secs_f64();
        let animation = matches!(self.workload.as_str(), "geometry" | "animation" | "both");
        let scroll = matches!(self.workload.as_str(), "scroll" | "both");
        column(cx).width_percent(100.).height_percent(100.).padding(24).gap(16) + |cx| {
            row(cx).height(100.).gap(18) + |cx| {
                for i in 0..3_usize {
                    cx.keyed(i, |cx| {
                        column(cx).width(220.).height(80.).padding(12).gap(12).radius(10.)
                            .background(Color::Panel).border(1., Color::Border) + |cx|
                        {
                            let value = if animation {
                                0.5 + 0.45 * (t * 2. + i as f64).sin()
                            } else {
                                0.5
                            };
                            column(cx).width((190. * value) as f32).height(8.).radius(4.)
                                .background(Color::Accent) + |_|
                            {};
                            row(cx).gap(6) + |cx| {
                                for dot in 0..8_usize {
                                    let opacity = if animation {
                                        0.25 + 0.75
                                            * (t * 5. - dot as f64).sin().max(0.)
                                    } else {
                                        1.
                                    };
                                    column(cx).key(dot).width(8.).height(8.).radius(4.)
                                        .opacity(opacity as f32).background(Color::Accent) + |_|
                                    {};
                                }
                            };
                        };
                    });
                }
            };
            if self.workload == "shader" {
                shader(cx, &EFFECT).width(680.).height(48.).animated(true);
            }
            column(cx).height(0.).grow(1.).scroll_y(true).gap(6).padding(12).background(Color::Panel).radius(12.)
            + |cx|
            {
                if scroll {
                    cx.scroll_to_y((160. + 140. * (t * 0.8).sin()) as f32);
                }
                for (i, label) in self.labels.iter().enumerate() {
                    cx.keyed(i, |cx| {
                        row(cx).height(24.).shrink(0.).gap(12).align(Align::Center) + |cx| {
                            column(cx).width(12.).height(12.).radius(3.).background(Color::Accent) + |_| {};
                            if self.workload != "geometry" {
                                text(cx, label.as_str()).font_size(15);
                            }
                        };
                    });
                }
            };
        };
    }
}
// Keep both renderers at a 60 Hz deadline, including uncapped Xvfb surfaces.
struct Paced {
    source: Source<Demo>,
    next: Instant,
    active: bool,
}
impl FrameSource for Paced {
    fn set_device_scale(&mut self, scale: f64) {
        self.source.set_device_scale(scale);
    }
    fn build_frame(
        &mut self,
        text: &mut TextContext,
        w: u32,
        h: u32,
    ) -> Vec<plico_renderer::layout::PlacedNode> {
        self.source.build_frame(text, w, h)
    }
    fn render_retained_frame(
        &mut self,
        scene: &mut Scene,
        text: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) -> Result<Option<(usize, f64, f64)>, plico_renderer::FrameSourceError> {
        let result = self.source.render_retained_frame(scene, text, size, scale);
        self.next = Instant::now() + Duration::from_secs_f64(1. / 60.);
        result
    }
    fn base_color(&self) -> peniko::Color {
        self.source.base_color()
    }
    fn handle_event(&mut self, event: plico_renderer::UiEvent) -> plico_renderer::EventResponse {
        self.source.handle_event(event)
    }
    fn animation_deadline(&self) -> Option<Instant> {
        self.active.then_some(self.next)
    }
    fn take_host_action(&mut self) -> Option<plico_renderer::HostAction> {
        self.source.take_host_action()
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse();
    let renderer = RenderBackend::Wgpu;
    if !["idle", "geometry", "animation", "scroll", "both", "shader"]
        .contains(&options.workload.as_str())
    {
        return Err("invalid workload".into());
    }
    let _trace = plico_shell::telemetry::init_tracing();
    tracing::info!(renderer=?renderer,workload=%options.workload,"probe.start");
    let active = options.workload != "idle";
    let app = Demo {
        workload: options.workload,
        labels: (0..40)
            .map(|i| format!("Row {i:02} · 中文 / 日本語 · cached desktop content"))
            .collect(),
        start: None,
        duration: Duration::from_secs(options.seconds),
        close: false,
    };
    plico_desktop::run_source_with_options(
        Paced {
            source: Source::new(app),
            next: Instant::now(),
            active,
        },
        WindowOptions::new()
            .title("Plico CPU fallback comparison")
            .renderer(renderer)
            .initial_inner_size(800, 700),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_workload_builds_valid_ui_across_clock_ticks() {
        for workload in ["idle", "geometry", "animation", "scroll", "both", "shader"] {
            let clock = plico::VirtualClock::default();
            let app = Demo {
                workload: workload.into(),
                labels: (0..40).map(|i| format!("Row {i}")).collect(),
                start: None,
                duration: Duration::from_secs(10),
                close: false,
            };
            let mut source = Source::with_clock(app, clock.clone());
            let mut text = TextContext::new();
            let mut scene = Scene::new();
            for _ in 0..3 {
                source
                    .render_retained_frame(&mut scene, &mut text, (800, 700), 1.)
                    .unwrap();
                assert!(source.adapter.placed.len() > 20);
                clock.advance(Duration::from_millis(17)).unwrap();
            }
        }
    }
}
