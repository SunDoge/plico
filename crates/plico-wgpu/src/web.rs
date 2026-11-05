//! Browser canvas presentation. Native window hosting remains in `window`.
use crate::{Error, Renderer, Result};
use plico_renderer::{Scene, TextContext};
use web_sys::HtmlCanvasElement;

#[derive(Clone, Copy, Debug)]
pub enum BrowserBackend {
    Auto,
    WebGpu,
    WebGl,
}
/// Retains its own color target; canvas/swapchain contents are never assumed preserved.
pub struct WebRenderer {
    canvas: HtmlCanvasElement,
    renderer: Renderer,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}
impl WebRenderer {
    pub async fn new(canvas: HtmlCanvasElement, backend: BrowserBackend) -> Result<Self> {
        let mut descriptor = crate::instance_descriptor();
        descriptor.backends = match backend {
            BrowserBackend::Auto => wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            BrowserBackend::WebGpu => wgpu::Backends::BROWSER_WEBGPU,
            BrowserBackend::WebGl => wgpu::Backends::GL,
        };
        let instance = match backend {
            BrowserBackend::Auto => {
                wgpu::util::new_instance_with_webgpu_detection(descriptor).await
            }
            _ => wgpu::Instance::new(descriptor),
        };
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| Error::Surface {
                message: e.to_string(),
            })?;
        let renderer = Renderer::new_for_surface(&instance, &surface).await?;
        let caps = renderer.surface_capabilities(&surface);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Srgb,
            width: canvas.width().max(1),
            height: canvas.height().max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        renderer.configure_surface(&surface, &config);
        Ok(Self {
            canvas,
            renderer,
            surface,
            config,
        })
    }
    pub fn backend_name(&self) -> &'static str {
        if self.renderer.adapter_info().backend == wgpu::Backend::Gl {
            "webgl2"
        } else {
            "webgpu"
        }
    }
    pub fn configure_text(&mut self, text: &mut TextContext) -> Result<()> {
        self.renderer.configure_web_text(text)
    }
    pub fn render(&mut self, scene: &Scene, size: [u32; 2], background: [f32; 4]) -> Result<bool> {
        if size.contains(&0) {
            return Ok(false);
        }
        if size != [self.config.width, self.config.height] {
            self.canvas.set_width(size[0]);
            self.canvas.set_height(size[1]);
            self.config.width = size[0];
            self.config.height = size[1];
            self.renderer.configure_surface(&self.surface, &self.config);
            self.renderer.invalidate_scene_damage();
        }
        let plan = self
            .renderer
            .plan_scene_frame(scene, size, background, true)?;
        if plan.unchanged() {
            return Ok(false);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            _ => {
                self.renderer.invalidate_scene_damage();
                return Err(Error::Surface {
                    message: "browser canvas surface unavailable".into(),
                });
            }
        };
        let view = frame.texture.create_view(&Default::default());
        self.renderer
            .submit_scene_plan(plan, &view, self.config.format)?;
        self.renderer.present(frame);
        Ok(true)
    }
}
