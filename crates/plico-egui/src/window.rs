//! Native window hosting: replay the scene through egui onto a wgpu surface.
//!
//! This uses the low-level `egui_wgpu::Renderer` plus the shell's
//! `wgpu_context` surface abstraction - the same pairing the Vello backend
//! uses - rather than `egui_wgpu::winit::Painter`, whose winit version does
//! not match the workspace's. Input stays in the shell (already translated
//! to `UiEvent`s); this window only paints.
use std::sync::Arc;

use egui::{Context, RawInput};
use plico_renderer::Scene;
use vello_common::kurbo::Affine;
use wgpu_context::{DeviceHandle, SurfaceRenderer, SurfaceRendererConfiguration, WGPUContext};
use winit::window::Window;

use crate::{GlyphCache, replay_scene_at};

/// An egui/epaint window renderer bound to one native window.
pub struct EguiWindow {
    context: Context,
    cache: GlyphCache,
    renderer: egui_wgpu::Renderer,
    surface: SurfaceRenderer<'static>,
    width: u32,
    height: u32,
    scale: f32,
    transparent: bool,
}

impl EguiWindow {
    /// Create the egui renderer for `window`. The scale factor is sampled
    /// once; the shell reports resizes through [`resize`](Self::resize).
    pub fn new(window: Arc<dyn Window>, transparent: bool) -> Self {
        // Mirror the Vello backend's surface setup: one wgpu context, a
        // compatible device, a Bgra8Unorm surface.
        let mut context = WGPUContext::with_features_and_limits(None, None);
        struct SharedWindow(Arc<dyn Window>);
        impl raw_window_handle::HasWindowHandle for SharedWindow {
            fn window_handle(
                &self,
            ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
            {
                self.0.window_handle()
            }
        }
        impl raw_window_handle::HasDisplayHandle for SharedWindow {
            fn display_handle(
                &self,
            ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
            {
                self.0.display_handle()
            }
        }
        // The shell reports dimensions through `resize`; the surface starts
        // at 1x1 until the first configure.
        let (width, height) = (1, 1);
        let owned: Arc<SharedWindow> = Arc::new(SharedWindow(window.clone()));
        let surface = context
            .create_surface(owned)
            .expect("failed to create egui window surface");
        let existing = context.find_compatible_device_handle(Some(&surface));
        let device = existing.unwrap_or_else(|| {
            pollster::block_on(DeviceHandle::new_from_compatible_surface(
                context.instance.clone(),
                Some(&surface),
                context.extra_features(),
                context.override_limits(),
            ))
            .expect("failed to create egui window device")
        });
        let surface_renderer = SurfaceRenderer::new(
            surface,
            SurfaceRendererConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                formats: vec![wgpu::TextureFormat::Bgra8Unorm],
                width,
                height,
                present_mode: wgpu::PresentMode::AutoNoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: if transparent {
                    wgpu::CompositeAlphaMode::PreMultiplied
                } else {
                    wgpu::CompositeAlphaMode::Opaque
                },
                view_formats: vec![],
            },
            None,
            device,
        )
        .expect("failed to configure egui window surface");
        let renderer = egui_wgpu::Renderer::new(
            surface_renderer.device(),
            egui_wgpu::wgpu::TextureFormat::Bgra8Unorm,
            egui_wgpu::RendererOptions::default(),
        );
        let scale = window.scale_factor() as f32;
        Self {
            context: Context::default(),
            cache: GlyphCache::default(),
            renderer,
            surface: surface_renderer,
            width,
            height,
            scale,
            transparent,
        }
    }

    /// Physical surface size changed.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.surface.resize(width, height);
    }

    /// Replay `scene` and present. Returns false when presentation failed,
    /// mirroring the Skia contract of "keep the last good frame".
    pub fn present(&mut self, scene: &mut Scene, background: peniko::Color) -> bool {
        let logical = egui::vec2(self.width as f32, self.height as f32) / self.scale;
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, logical)),
            ..Default::default()
        };
        self.context.begin_pass(input);
        let painter = self.context.layer_painter(egui::LayerId::background());
        let transform = Affine::scale(f64::from(self.scale));
        if let Err(error) = replay_scene_at(&painter, scene, &mut self.cache, transform) {
            // Unsupported commands fail loudly in tests; a window host
            // keeps the last good frame instead of blanking on a partial
            // scene, and the rejection stays observable.
            tracing::warn!(%error, "egui replay rejected a command; skipping the frame");
        }
        drop(painter);
        let mut output = self.context.end_pass();

        // Upload pending glyph/shadow textures created by the replay, then
        // drain: epaint asserts the delta is fully applied when dropped.
        let mut pending = std::mem::take(&mut output.textures_delta.set);
        for (id, deltas) in pending.drain() {
            let (device, queue) = (self.surface.device(), self.surface.queue());
            for delta in deltas {
                self.renderer.update_texture(device, queue, id, &delta);
            }
        }
        // Free replaced textures after painting; epaint asserts that the
        // delta is fully applied when dropped.
        let mut freed = std::mem::take(&mut output.textures_delta.free);
        for id in freed.drain() {
            self.renderer.free_texture(&id);
        }
        let shape_count = output.shapes.len();
        let clipped = self.context.tessellate(output.shapes, self.scale);
        tracing::info!(
            shapes = shape_count,
            meshes = clipped.len(),
            "egui frame tessellated"
        );
        let descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.width, self.height],
            pixels_per_point: self.scale,
        };
        let Ok(view) = self.surface.target_texture_view() else {
            // epaint panics on unapplied texture deltas at drop; clear them
            // before the early return keeps the last good frame.
            output.textures_delta.clear();
            self.surface.clear_surface_texture();
            return false;
        };
        let device = self.surface.device();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("plico egui frame"),
        });
        let clear = if self.transparent {
            wgpu::LoadOp::Clear(wgpu::Color {
                r: background.components[0] as f64,
                g: background.components[1] as f64,
                b: background.components[2] as f64,
                a: background.components[3] as f64,
            })
        } else {
            wgpu::LoadOp::Clear(wgpu::Color {
                r: background.components[0] as f64,
                g: background.components[1] as f64,
                b: background.components[2] as f64,
                a: 1.,
            })
        };
        // egui's tessellation buffer uploads happen before the pass so the
        // pass can bind them.
        let (device, queue) = (self.surface.device(), self.surface.queue());
        let buffers =
            self.renderer
                .update_buffers(device, queue, &mut encoder, &clipped, &descriptor);
        for buffer in buffers {
            queue.submit([buffer]);
        }
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("plico egui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: clear,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // The 'static requirement is bridged with the documented
            // forget_lifetime; the pass ends in this scope.
            self.renderer
                .render(&mut pass.forget_lifetime(), &clipped, &descriptor);
        }
        queue.submit([encoder.finish()]);
        self.surface.maybe_blit_and_present().is_ok()
    }
}
