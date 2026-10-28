//! Native egui controls hosted by the shared Plico window shell.
//!
//! Text and controls use egui; only GPU submission uses the low-level `egui_wgpu::Renderer` plus the shell's
//! `wgpu_context` surface abstraction - the same pairing the Vello backend
//! uses - rather than `egui_wgpu::winit::Painter`, whose winit version does
//! not match the workspace's. Input stays in the shell (already translated
//! to `UiEvent`s); this window only paints.
use std::sync::Arc;

use egui::{Context, RawInput};
use plico_renderer::Scene;
use plico_renderer::{EventResponse, FrameSource, UiEvent};
use std::collections::VecDeque;
use wgpu_context::{DeviceHandle, SurfaceRenderer, SurfaceRendererConfiguration, WGPUContext};
use winit::window::Window;

use crate::Adapter;

/// An egui/epaint window renderer bound to one native window.
pub struct EguiWindow {
    context: Context,
    adapter: Adapter,
    input: RawInput,
    output: Option<egui::FullOutput>,
    actions: VecDeque<plico_renderer::HostAction>,
    ime: Option<plico_renderer::ImeState>,
    pointer_cursor: plico_renderer::style::CursorStyle,
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
    pub fn new(window: Arc<dyn Window>, transparent: bool) -> Result<Self, crate::Error> {
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
            .map_err(|error| crate::Error::Window {
                message: error.to_string(),
            })?;
        let device = if let Some(device) = context.find_compatible_device_handle(Some(&surface)) {
            device
        } else {
            pollster::block_on(DeviceHandle::new_from_compatible_surface(
                context.instance.clone(),
                Some(&surface),
                context.extra_features(),
                context.override_limits(),
            ))
            .map_err(|error| crate::Error::Window {
                message: error.to_string(),
            })?
        };
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
        .map_err(|error| crate::Error::Window {
            message: error.to_string(),
        })?;
        let renderer = egui_wgpu::Renderer::new(
            surface_renderer.device(),
            surface_renderer.config.format,
            egui_wgpu::RendererOptions::default(),
        );
        let scale = window.scale_factor() as f32;
        let context = Adapter::new_context();
        context.set_request_repaint_callback(move |request| {
            if request.delay.is_zero() {
                window.request_redraw();
            }
        });
        Ok(Self {
            context,
            adapter: Adapter::default(),
            input: RawInput::default(),
            output: None,
            actions: VecDeque::new(),
            ime: None,
            pointer_cursor: Default::default(),
            renderer,
            surface: surface_renderer,
            width,
            height,
            scale,
            transparent,
        })
    }

    pub fn build(
        &mut self,
        source: &mut dyn FrameSource,
        size: [u32; 2],
        scale: f64,
    ) -> Option<(usize, f64, f64)> {
        self.scale = scale as f32;
        let start = std::time::Instant::now();
        let engine = source.native_widget_frame([size[0] as f32, size[1] as f32])?;
        if let Err(error) = Adapter::validate(engine, engine.root()) {
            tracing::error!(%error,"egui native frame rejected before drawing");
            return None;
        }
        let declared = start.elapsed().as_secs_f64() * 1000.;
        let mut input = self.input.take();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size[0] as f32, size[1] as f32),
        ));
        input.time = Some(engine.time().now.as_secs_f64());
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(self.scale);
        if engine.theme().surface < 0x808080 {
            self.context.set_visuals(egui::Visuals::dark());
        } else {
            self.context.set_visuals(egui::Visuals::light());
        }
        self.context.begin_pass(input);
        let result = self
            .adapter
            .show(&self.context, engine, [size[0] as f32, size[1] as f32]);
        let mut output = self.context.end_pass();
        let changed = match result {
            Ok(changed) => changed,
            Err(error) => {
                tracing::error!(%error, "egui native frame rejected");
                output.textures_delta.clear();
                return None;
            }
        };
        for command in &output.platform_output.commands {
            match command {
                egui::OutputCommand::CopyText(text) => self
                    .actions
                    .push_back(plico_renderer::HostAction::SetClipboard(text.clone())),
                egui::OutputCommand::OpenUrl(url) => self
                    .actions
                    .push_back(plico_renderer::HostAction::OpenUrl(url.url.clone())),
                _ => {}
            }
        }
        self.ime = output
            .platform_output
            .ime
            .map(|ime| plico_renderer::ImeState {
                client_id: self
                    .context
                    .memory(|m| m.focused())
                    .map_or(1, |id| id.value()),
                cursor_area: [
                    ime.cursor_rect.min.x as f64,
                    ime.cursor_rect.min.y as f64,
                    ime.cursor_rect.max.x as f64,
                    ime.cursor_rect.max.y as f64,
                ],
                surrounding_text: String::new(),
                surrounding_cursor: 0,
                surrounding_anchor: 0,
                selection_utf16: 0..0,
                selection_reversed: false,
                marked_range_utf16: None,
                purpose: plico_renderer::WidgetImePurpose::Normal,
                multiline: false,
                completion: true,
                spellcheck: true,
            });
        self.pointer_cursor = match output.platform_output.cursor_icon {
            egui::CursorIcon::PointingHand => plico_renderer::style::CursorStyle::Pointer,
            egui::CursorIcon::Text => plico_renderer::style::CursorStyle::Text,
            _ => Default::default(),
        };
        let repaint_delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.output = Some(output);
        if changed {
            source.native_widget_input_changed();
        }
        source.native_widget_repaint_after(repaint_delay);
        Some((
            self.adapter.node_count(),
            declared,
            start.elapsed().as_secs_f64() * 1000. - declared,
        ))
    }
    pub fn take_host_action(&mut self) -> Option<plico_renderer::HostAction> {
        self.actions.pop_front()
    }
    pub fn ime_state(&self) -> Option<plico_renderer::ImeState> {
        self.ime.clone()
    }
    pub fn pointer_cursor(&self) -> plico_renderer::style::CursorStyle {
        self.pointer_cursor
    }
    pub fn event(&mut self, event: &UiEvent) -> Option<EventResponse> {
        use plico_renderer::{ImeEvent, PointerButton, PointerPhase};
        let mut response = EventResponse {
            handled: true,
            request_redraw: true,
            ..Default::default()
        };
        match event {
            UiEvent::Pointer(p) => {
                let pos = egui::pos2(p.position.x as f32, p.position.y as f32);
                match p.phase {
                    PointerPhase::Move | PointerPhase::Enter => {
                        self.input.events.push(egui::Event::PointerMoved(pos))
                    }
                    PointerPhase::Leave | PointerPhase::Cancel => {
                        self.input.events.push(egui::Event::PointerGone)
                    }
                    PointerPhase::Down | PointerPhase::Up => {
                        let button = match p.button? {
                            PointerButton::Primary => egui::PointerButton::Primary,
                            PointerButton::Secondary => egui::PointerButton::Secondary,
                            PointerButton::Auxiliary => egui::PointerButton::Middle,
                            _ => return None,
                        };
                        self.input.events.push(egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: p.phase == PointerPhase::Down,
                            modifiers: modifiers(p.modifiers),
                        });
                    }
                }
            }
            UiEvent::Wheel(w) => self.input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                delta: egui::vec2(-w.delta_x as f32, -w.delta_y as f32),
                modifiers: modifiers(w.modifiers),
            }),
            UiEvent::Key(key) => {
                let mods = modifiers(key.modifiers);
                self.input.events.push(egui::Event::ModifiersChanged(mods));
                let down = key.phase == plico::KeyPhase::Down;
                if down && key.modifiers.primary_shortcut() {
                    match key.key.to_lowercase().as_str() {
                        "v" => {
                            response.clipboard = Some(plico_renderer::ClipboardRequest::Read);
                            response.consume_key_text = true;
                            return Some(response);
                        }
                        "c" => self.input.events.push(egui::Event::Copy),
                        "x" => self.input.events.push(egui::Event::Cut),
                        _ => {}
                    }
                }
                let key_name = match key.key.as_str() {
                    " " => "Space",
                    s => s,
                };
                if let Some(k) = egui::Key::from_name(key_name) {
                    self.input.events.push(egui::Event::Key {
                        key: k,
                        physical_key: None,
                        pressed: down,
                        repeat: key.repeat,
                        modifiers: mods,
                    });
                }
                response.handled = self.context.egui_wants_keyboard_input();
            }
            UiEvent::TextInput(text) => self.input.events.push(egui::Event::Text(text.clone())),
            UiEvent::Paste(text) => self.input.events.push(egui::Event::Paste(text.clone())),
            UiEvent::Ime(ime) => self.input.events.push(egui::Event::Ime(match ime {
                ImeEvent::Enabled => return Some(response),
                ImeEvent::Disabled => egui::ImeEvent::Preedit {
                    text: String::new(),
                    active_range_chars: None,
                },
                ImeEvent::Preedit { text, cursor } => egui::ImeEvent::Preedit {
                    text: text.clone(),
                    active_range_chars: cursor.map(|(a, b)| {
                        text.get(..a).map_or(0, |s| s.chars().count())
                            ..text.get(..b).map_or(0, |s| s.chars().count())
                    }),
                },
                ImeEvent::Commit(text) => egui::ImeEvent::Commit(text.clone()),
                _ => return None,
            })),
            UiEvent::Focus(focus) => {
                self.input.focused = *focus;
                self.input.events.push(egui::Event::WindowFocused(*focus));
            }
            UiEvent::ModifiersChanged(mods) => self
                .input
                .events
                .push(egui::Event::ModifiersChanged(modifiers(*mods))),
            _ => return None,
        }
        Some(response)
    }

    /// Physical surface size changed.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.surface.resize(width, height);
    }

    /// Replay `scene` and present. Returns false when presentation failed,
    /// mirroring the Skia contract of "keep the last good frame".
    pub fn present(&mut self, _scene: &mut Scene, background: peniko::Color) -> bool {
        let Some(mut output) = self.output.take() else {
            return false;
        };
        // Upload egui's font atlas and image deltas, then
        // drain: epaint asserts the delta is fully applied when dropped.
        let mut pending = std::mem::take(&mut output.textures_delta.set);
        for (id, deltas) in pending.drain() {
            let (device, queue) = (self.surface.device(), self.surface.queue());
            for delta in deltas {
                self.renderer.update_texture(device, queue, id, &delta);
            }
        }
        let freed = std::mem::take(&mut output.textures_delta.free);
        let shape_count = output.shapes.len();
        let clipped = self.context.tessellate(output.shapes, self.scale);
        tracing::trace!(
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
            for id in freed {
                self.renderer.free_texture(&id);
            }
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
        let presented = self.surface.maybe_blit_and_present().is_ok();
        for id in freed {
            self.renderer.free_texture(&id);
        }
        presented
    }
}

fn modifiers(m: plico::Modifiers) -> egui::Modifiers {
    egui::Modifiers {
        alt: m.alt(),
        ctrl: m.control(),
        shift: m.shift(),
        mac_cmd: m.meta(),
        command: if cfg!(target_os = "macos") {
            m.meta()
        } else {
            m.control()
        },
    }
}
