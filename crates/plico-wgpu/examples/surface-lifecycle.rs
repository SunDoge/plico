//! Main-thread native window exercise; requires `--features window`.
//! Reported latency is the CPU duration of successful present() calls, not FPS.
#[path = "surface-lifecycle/memory.rs"]
mod memory;

use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene, WakeCallback};
use plico_wgpu::WgpuWindow;
use raw_window_handle::HasWindowHandle;
use snafu::Snafu;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

#[derive(Debug, Snafu)]
#[snafu(display("native surface check failed: {message}"))]
struct CheckError {
    message: String,
}
fn error(value: impl std::fmt::Display) -> CheckError {
    CheckError {
        message: value.to_string(),
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Initial,
    Warmup,
    Resize,
    Idle,
    AfterIdle,
    Hidden,
    AfterHide,
    Done,
}
impl Phase {
    fn draws(self) -> bool {
        matches!(
            self,
            Self::Initial | Self::Warmup | Self::Resize | Self::AfterIdle | Self::AfterHide
        )
    }
}
struct Native {
    window: Arc<dyn Window>,
    gpu: WgpuWindow,
}
struct Probe {
    native: Option<Native>,
    wake: WakeCallback,
    outcome: Arc<Mutex<Option<Result<String, CheckError>>>>,
    phase: Phase,
    frame: u32,
    deadline: Option<Instant>,
    watchdog: Instant,
    warmup_start: Option<Instant>,
    warmup_elapsed: Duration,
    samples: Vec<Duration>,
    pending: u32,
    idle_ready: u32,
    old_extent: [u32; 2],
}
impl Probe {
    fn fail(&mut self, event_loop: &dyn ActiveEventLoop, failure: CheckError) {
        *self.outcome.lock().unwrap() = Some(Err(failure));
        self.phase = Phase::Done;
        event_loop.exit();
    }
    fn draw(&mut self, event_loop: &dyn ActiveEventLoop) -> Result<(), CheckError> {
        if !self.phase.draws() {
            return Ok(());
        }
        let native = self
            .native
            .as_mut()
            .ok_or_else(|| error("missing native window"))?;
        let extent = native.window.surface_size();
        if extent.width == 0 || extent.height == 0 {
            return Ok(());
        }
        if self.phase == Phase::Resize && [extent.width, extent.height] == self.old_extent {
            return Ok(());
        }
        let mut scene = Scene::new();
        let x = 12. + f64::from(self.frame % 48) * 2.;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(210, 60, 40),
            None,
            &Rect::new(x, 24., x + 50., 74.),
        );
        let started = Instant::now();
        let accepted = if self.phase == Phase::Initial {
            native.gpu.present_initial(&scene, Color::BLACK)
        } else {
            native.gpu.present(&scene, Color::BLACK)
        }
        .map_err(error)?;
        let elapsed = started.elapsed();
        if !accepted {
            if native.gpu.surface_acquire_pending() {
                self.pending += 1;
            } else {
                self.deadline = Some(Instant::now() + Duration::from_millis(16));
            }
            return Ok(());
        }
        if native.gpu.last_present_skipped() {
            return Err(error("moving scene unexpectedly skipped"));
        }
        self.frame += 1;
        match self.phase {
            Phase::Initial => {
                memory::report("initial")?;
                self.phase = Phase::Warmup;
                self.warmup_start = Some(Instant::now());
                native.window.focus_window();
                native.window.request_redraw();
            }
            Phase::Warmup => {
                self.samples.push(elapsed);
                if self.samples.len() < 60 {
                    native.window.request_redraw();
                } else {
                    self.warmup_elapsed = self.warmup_start.unwrap().elapsed();
                    memory::report("warmup")?;
                    self.phase = Phase::Resize;
                    self.old_extent = [extent.width, extent.height];
                    for size in [(520., 350.), (580., 360.), (540., 340.)] {
                        let _ = native
                            .window
                            .request_surface_size(LogicalSize::new(size.0, size.1).into());
                    }
                    native.window.request_redraw();
                }
            }
            Phase::Resize => {
                memory::report("resized")?;
                self.phase = Phase::Idle;
                self.deadline = Some(
                    native
                        .gpu
                        .maintenance_deadline()
                        .ok_or_else(|| error("idle cleanup not armed"))?,
                );
            }
            Phase::AfterIdle => {
                memory::report("idle-recovered")?;
                if native.gpu.maintenance_deadline().is_none() {
                    return Err(error("idle recovery did not rearm cleanup"));
                }
                self.phase = Phase::Hidden;
                native.gpu.prepare_hide();
                native.window.set_visible(false);
                native.gpu.trim_gpu_cache();
                self.deadline = Some(Instant::now() + Duration::from_millis(250));
            }
            Phase::AfterHide => {
                memory::report("hide-recovered")?;
                let (captured_size, pixels) = native
                    .gpu
                    .read_retained_rgba()
                    .map_err(error)?
                    .ok_or_else(|| error("recovery presented without a retained target"))?;
                let width = captured_size[0] as usize;
                let edge = (40 * width + x as usize + 49) * 4;
                if captured_size != [extent.width, extent.height]
                    || pixels.len() != width * captured_size[1] as usize * 4
                    || pixels.get(..4) != Some(&[0, 0, 0, 255])
                    || pixels.get(edge..edge + 4) != Some(&[210, 60, 40, 255])
                {
                    return Err(error(
                        "retained recovery pixels do not match the latest submitted scene",
                    ));
                }
                if self.idle_ready != 0 {
                    return Err(error("idle prefetch produced a readiness redraw"));
                }
                self.samples.sort_unstable();
                let p95 = self.samples[(self.samples.len() * 95).div_ceil(100).saturating_sub(1)];
                let mode = if native.gpu.surface_prefetch_enabled() {
                    "async"
                } else {
                    "sync"
                };
                let backend = match native.window.window_handle().map_err(error)?.as_raw() {
                    raw_window_handle::RawWindowHandle::Wayland(_) => "wayland",
                    raw_window_handle::RawWindowHandle::Xlib(_)
                    | raw_window_handle::RawWindowHandle::Xcb(_) => "x11",
                    raw_window_handle::RawWindowHandle::AppKit(_) => "appkit",
                    _ => "other",
                };
                let summary = format!(
                    "surface-lifecycle: PASS mode={mode} backend={backend} frames={} pending={} present_p95_us={} warmup_ms={} idle_ready={}",
                    self.frame,
                    self.pending,
                    p95.as_micros(),
                    self.warmup_elapsed.as_millis(),
                    self.idle_ready
                );
                *self.outcome.lock().unwrap() = Some(Ok(summary));
                self.phase = Phase::Done;
                native.gpu.trim_gpu_cache();
                event_loop.exit();
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.native.is_some() || self.phase == Phase::Done {
            return;
        }
        let created = (|| {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Plico native surface lifecycle check")
                            .with_surface_size(LogicalSize::new(640., 420.))
                            .with_visible(false),
                    )
                    .map_err(error)?,
            );
            let mut gpu = WgpuWindow::new(window.clone(), false).map_err(error)?;
            gpu.set_surface_wake(self.wake.clone());
            #[cfg(target_os = "macos")]
            if !matches!(
                std::env::var("PLICO_WGPU_ASYNC_SURFACE").ok().as_deref(),
                Some("0" | "false")
            ) && !gpu.surface_prefetch_enabled()
            {
                return Err(error(
                    "background acquisition requested but worker unavailable",
                ));
            }
            Ok::<_, CheckError>(Native { window, gpu })
        })();
        match created {
            Ok(native) => {
                native.window.request_redraw();
                self.native = Some(native);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if self.phase == Phase::Done {
            return;
        }
        match event {
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::SurfaceResized(size) => {
                if let Some(native) = &mut self.native {
                    native.gpu.resize(size.width, size.height);
                    if self.phase.draws() {
                        native.window.request_redraw();
                    }
                }
            }
            WindowEvent::CloseRequested => {
                self.fail(event_loop, error("window closed before completion"))
            }
            _ => {}
        }
    }
    fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) {
        if let Some(native) = &self.native
            && native.gpu.take_surface_ready()
        {
            if self.phase.draws() {
                native.window.request_redraw();
            } else {
                self.idle_ready += 1;
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.phase == Phase::Done {
            return;
        }
        let now = Instant::now();
        if now >= self.watchdog && self.phase != Phase::Done {
            self.fail(event_loop, error("event-loop progress exceeded 20s"));
            return;
        }
        if self.deadline.is_some_and(|deadline| deadline <= now) {
            self.deadline = None;
            if let Some(native) = &mut self.native {
                match self.phase {
                    Phase::Idle => {
                        native.gpu.maintain(now);
                        if native.gpu.maintenance_deadline().is_some() {
                            self.fail(event_loop, error("idle maintenance was not consumed"));
                            return;
                        }
                        if let Err(error) = memory::report("idle-trimmed") {
                            self.fail(event_loop, error);
                            return;
                        }
                        self.phase = Phase::AfterIdle;
                        native.window.request_redraw();
                    }
                    Phase::Hidden => {
                        if let Err(error) = memory::report("hidden-trimmed") {
                            self.fail(event_loop, error);
                            return;
                        }
                        self.phase = Phase::AfterHide;
                        native.gpu.invalidate_presentation();
                        native.window.set_visible(true);
                        native.window.focus_window();
                        native.window.request_redraw();
                    }
                    _ if self.phase.draws() => native.window.request_redraw(),
                    _ => {}
                }
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            self.deadline
                .map_or(self.watchdog, |deadline| deadline.min(self.watchdog)),
        ));
    }
}
fn main() -> Result<(), CheckError> {
    memory::report("startup")?;
    // Use the same backend selection as applications, including native Wayland.
    let event_loop = EventLoop::builder().build().map_err(error)?;
    let proxy = event_loop.create_proxy();
    let outcome = Arc::new(Mutex::new(None));
    event_loop
        .run_app(Probe {
            native: None,
            wake: Arc::new(move || proxy.wake_up()),
            outcome: outcome.clone(),
            phase: Phase::Initial,
            frame: 0,
            deadline: None,
            watchdog: Instant::now() + Duration::from_secs(20),
            warmup_start: None,
            warmup_elapsed: Duration::ZERO,
            samples: Vec::new(),
            pending: 0,
            idle_ready: 0,
            old_extent: [0; 2],
        })
        .map_err(error)?;
    let result = outcome
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| error("event loop ended without a completed check"))?;
    println!("{}", result?);
    Ok(())
}
