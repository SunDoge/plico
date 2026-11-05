//! D: theme; 1/2/3/4: 100/125/150/200% UI scale; wheel: scroll;
//! Space: animate scrolling; C: toggle continuous versus pixel-snapped text.
#[cfg(all(target_os = "linux", feature = "system-text"))]
mod support {
    pub mod text_page;
}
#[cfg(all(target_os = "linux", feature = "system-text"))]
mod app {
    use super::support::text_page::Page;
    use plico_wgpu::Renderer;
    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
    };
    use std::{sync::Arc, time::Instant};
    use winit::{
        application::ApplicationHandler,
        dpi::LogicalSize,
        event::{ElementState, MouseScrollDelta, WindowEvent},
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        keyboard::{Key, NamedKey},
        window::{Window, WindowAttributes, WindowId},
    };
    struct SharedWindow(Arc<dyn Window>);
    impl HasWindowHandle for SharedWindow {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            self.0.window_handle()
        }
    }
    impl HasDisplayHandle for SharedWindow {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            self.0.display_handle()
        }
    }
    struct State {
        window: Arc<dyn Window>,
        renderer: Renderer,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        page: Page,
        scale: f64,
        dark: bool,
        scroll: f64,
        continuous: bool,
        animate: bool,
        start: Instant,
        frames: usize,
    }
    impl State {
        fn new(event_loop: &dyn ActiveEventLoop) -> Result<Self, Box<dyn std::error::Error>> {
            let window: Arc<dyn Window> = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Plico wgpu — system text")
                        .with_surface_size(LogicalSize::new(840., 660.))
                        .with_min_surface_size(LogicalSize::new(500., 400.)),
                )?
                .into();
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
            let surface = instance.create_surface(SharedWindow(window.clone()))?;
            let mut renderer = pollster::block_on(Renderer::new_for_surface(&instance, &surface))?;
            let caps = renderer.surface_capabilities(&surface);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| {
                    matches!(
                        f,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                    )
                })
                .unwrap_or(caps.formats[0]);
            let size = window.surface_size();
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                color_space: wgpu::SurfaceColorSpace::Srgb,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
            };
            renderer.configure_surface(&surface, &config);
            let scale = window.scale_factor();
            let page = Page::new(&mut renderer, scale, false)?;
            println!(
                "window adapter={:?} format={format:?} scale={scale}",
                renderer.adapter_info()
            );
            Ok(Self {
                window,
                renderer,
                surface,
                config,
                page,
                scale,
                dark: false,
                scroll: 0.,
                continuous: false,
                animate: false,
                start: Instant::now(),
                frames: 0,
            })
        }
        fn rebuild(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            self.page = Page::new(&mut self.renderer, self.scale, self.dark)?;
            println!(
                "rebuilt text: scale={} dark={} continuous={}",
                self.scale, self.dark, self.continuous
            );
            self.window.request_redraw();
            Ok(())
        }
        fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            let size = self.window.surface_size();
            if size.width == 0 || size.height == 0 {
                return Ok(());
            }
            if self.config.width != size.width || self.config.height != size.height {
                self.config.width = size.width;
                self.config.height = size.height;
                self.renderer.configure_surface(&self.surface, &self.config);
            }
            let frame = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                wgpu::CurrentSurfaceTexture::Outdated => {
                    self.renderer.configure_surface(&self.surface, &self.config);
                    self.window.request_redraw();
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(());
                }
                other => return Err(format!("surface acquisition failed: {other:?}").into()),
            };
            let offset = -self.scroll * self.scale
                + if self.animate {
                    (self.start.elapsed().as_secs_f64() * 1.3).sin() * 30. * self.scale
                } else {
                    0.
                };
            let mut quads = self.page.quads.clone();
            for q in &mut quads {
                q.rect[1] += offset as f32;
            }
            let draws = self.page.draws(&quads, offset, self.continuous);
            self.renderer.render_to_view(
                &draws,
                [size.width, size.height],
                self.page.background,
                self.start.elapsed().as_secs_f32(),
                &frame.texture.create_view(&Default::default()),
                self.config.format,
            )?;
            self.window.pre_present_notify();
            self.renderer.present(frame);
            self.frames += 1;
            if self.frames == 1 {
                println!(
                    "presented first frame; text stats={:?}",
                    self.renderer.text_stats()
                );
            }
            if self.animate {
                self.window.request_redraw();
            }
            Ok(())
        }
    }
    #[derive(Default)]
    struct App {
        state: Option<State>,
    }
    impl ApplicationHandler for App {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            if self.state.is_some() {
                return;
            }
            match State::new(event_loop) {
                Ok(state) => {
                    state.window.request_redraw();
                    self.state = Some(state);
                }
                Err(e) => {
                    eprintln!("window initialization failed: {e}");
                    event_loop.exit();
                }
            }
        }
        fn destroy_surfaces(&mut self, _: &dyn ActiveEventLoop) {
            self.state = None;
        }
        fn window_event(
            &mut self,
            event_loop: &dyn ActiveEventLoop,
            _: WindowId,
            event: WindowEvent,
        ) {
            let Some(state) = &mut self.state else {
                return;
            };
            let result = match event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                    return;
                }
                WindowEvent::RedrawRequested => state.draw(),
                WindowEvent::SurfaceResized(_) => {
                    state.window.request_redraw();
                    Ok(())
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    state.scale = scale_factor;
                    state.rebuild()
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    state.scroll += match delta {
                        MouseScrollDelta::LineDelta(_, y) => -f64::from(y) * 24.,
                        MouseScrollDelta::PixelDelta(p) => -p.y / state.scale,
                        _ => 0.,
                    };
                    state.window.request_redraw();
                    Ok(())
                }
                WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed =>
                {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => event_loop.exit(),
                        Key::Character(key) => match key.as_str() {
                            " " => {
                                state.animate = !state.animate;
                                state.window.request_redraw();
                            }
                            "d" | "D" => {
                                state.dark = !state.dark;
                            }
                            "c" | "C" => {
                                state.continuous = !state.continuous;
                            }
                            "1" => state.scale = state.window.scale_factor(),
                            "2" => state.scale = state.window.scale_factor() * 1.25,
                            "3" => state.scale = state.window.scale_factor() * 1.5,
                            "4" => state.scale = state.window.scale_factor() * 2.,
                            _ => {}
                        },
                        _ => {}
                    }
                    state.rebuild()
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                eprintln!("window rendering failed: {e}");
                event_loop.exit();
            }
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop.run_app(App::default())?;
        Ok(())
    }
}
#[cfg(all(target_os = "linux", feature = "system-text"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
#[cfg(not(all(target_os = "linux", feature = "system-text")))]
fn main() {
    eprintln!("The system-text window currently requires Linux and system-text.");
}
