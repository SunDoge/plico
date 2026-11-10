//! Reusable window + Vello GPU backend + scene + text context.
//!
//! Extracted from the app so multiple hosts share one windowing setup. A host
//! provides a [`crate::source::FrameSource`] that fills the scene each frame;
//! [`Shell`] handles presentation + vsync + resize.

#![warn(missing_docs)]

use snafu::ResultExt;
use std::sync::Arc;

use crate::Scene;
use peniko::Color;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

use crate::accessibility::AccessibilityState;
use crate::source::{WindowInputMode, WindowLevel, WindowOptions};
use crate::text::TextContext;
use crate::window_renderer::WindowRenderer;
use plico_renderer::platform::WindowBackground;

/// Native rendering resources owned by one live window.
pub struct Shell {
    // Invalidate before the window/view is dropped (fields drop in order).
    #[cfg(target_os = "macos")]
    frame_clock: Option<crate::macos_frame_clock::NativeFrameClock>,
    /// Platform window handle.
    pub window: Arc<dyn Window>,
    /// Selected graphics consumer for this window.
    pub(crate) renderer: WindowRenderer,
    /// Current physical render size.
    surface_size: [u32; 2],
    corner_radius: u32,
    deferred_show: bool,
    #[cfg(target_os = "macos")]
    native_frame: crate::macos_window_frame::NativeWindowFrame,
    #[cfg(target_os = "macos")]
    native_input: crate::macos_input::NativeInput,
    #[cfg(target_os = "macos")]
    preferences: crate::macos_preferences::NativePreferences,
    #[cfg(target_os = "linux")]
    preferences: crate::linux_preferences::NativePreferences,
    presentation_error: Option<String>,
    /// Reused backend-neutral application scene for the current frame.
    pub scene: Scene,
    /// Shared text shaping and retained glyph resources.
    pub tcx: TextContext,
    /// Platform accessibility adapter for the window.
    pub accessibility: AccessibilityState,
}

impl Shell {
    /// Create the window, selected graphics backend and a fresh
    /// [`TextContext`]. Returns a typed error when any window/GPU step fails.
    pub fn create(
        event_loop: &dyn ActiveEventLoop,
        options: &WindowOptions,
    ) -> crate::Result<Shell> {
        let options = options.clone().normalized();
        let corner_radius =
            crate::window_frame::client_corner_radius(options.corner_radius, options.decorations);
        let transparent = options.background != WindowBackground::Opaque || corner_radius > 0;
        if matches!(
            options.background,
            WindowBackground::Blurred | WindowBackground::Mica | WindowBackground::MicaAlt
        ) {
            tracing::warn!(
                background = ?options.background,
                "native compositor material is not implemented; using a transparent surface"
            );
        }
        let mut attrs = WindowAttributes::default()
            .with_title(options.title.clone())
            .with_visible(false)
            .with_active(options.activate_on_show)
            .with_resizable(options.resizable)
            .with_decorations(options.decorations)
            .with_transparent(transparent)
            .with_window_level(match options.window_level {
                WindowLevel::AlwaysOnBottom => winit::window::WindowLevel::AlwaysOnBottom,
                WindowLevel::Normal => winit::window::WindowLevel::Normal,
                WindowLevel::AlwaysOnTop => winit::window::WindowLevel::AlwaysOnTop,
            })
            .with_surface_size(winit::dpi::LogicalSize::new(
                options.initial_inner_size.0,
                options.initial_inner_size.1,
            ));
        #[cfg(target_os = "macos")]
        if options.titlebar_overlay && options.decorations {
            // Set platform attributes after the common decoration choice:
            // decorations override these AppKit titlebar properties otherwise.
            use winit::platform::macos::WindowAttributesMacOS;
            attrs = attrs.with_platform_attributes(Box::new(
                WindowAttributesMacOS::default()
                    .with_titlebar_transparent(true)
                    .with_title_hidden(true)
                    .with_fullsize_content_view(true)
                    .with_movable_by_window_background(false),
            ));
        }
        if let Some((x, y)) = options.initial_position {
            attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x, y));
        }
        if options.maximized {
            attrs = attrs.with_maximized(true);
        }
        if options.fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::monitor::Fullscreen::Borderless(None)));
        }
        if let Some((width, height)) = options.min_inner_size {
            attrs = attrs.with_min_surface_size(winit::dpi::LogicalSize::new(width, height));
        }
        if let Some((width, height)) = options.max_inner_size {
            attrs = attrs.with_max_surface_size(winit::dpi::LogicalSize::new(width, height));
        }
        if options.centered
            && let Some(monitor) = event_loop
                .primary_monitor()
                .or_else(|| event_loop.available_monitors().next())
            && let (Some(origin), Some(mode)) = (monitor.position(), monitor.current_video_mode())
        {
            let scale = monitor.scale_factor().max(f64::EPSILON);
            let width = (f64::from(options.initial_inner_size.0) * scale).round() as i32;
            let height = (f64::from(options.initial_inner_size.1) * scale).round() as i32;
            let size = mode.size();
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
                origin.x + (size.width as i32 - width).max(0) / 2,
                origin.y + (size.height as i32 - height).max(0) / 2,
            ));
        }

        let window: Arc<dyn Window> = Arc::from(
            event_loop
                .create_window(attrs)
                .context(crate::error::CreateWindowSnafu)?,
        );
        let physical_size = window.surface_size();
        let accessibility = AccessibilityState::new(window.clone(), options.title.clone());
        if options.input_mode == WindowInputMode::Passthrough
            && let Err(error) = window.set_cursor_hittest(false)
        {
            tracing::warn!(?error, "pointer passthrough is unavailable for this window");
        }
        let surface_width = physical_size.width.max(1);
        let surface_height = physical_size.height.max(1);
        let mut renderer = WindowRenderer::new(options.renderer, transparent)?;
        renderer.resume(window.clone(), surface_width, surface_height)?;
        let mut tcx = TextContext::new();
        renderer.configure_text(&mut tcx)?;
        tracing::info!(
            text_backend = tcx.raster_backend_name(),
            "Window text layout initialized"
        );
        // Apply native clipping after Metal creates its surface layer, before
        // exposing the window. Core Animation owns the continuous window contour.
        #[cfg(target_os = "macos")]
        let mut native_frame = crate::macos_window_frame::NativeWindowFrame::default();
        #[cfg(target_os = "macos")]
        if options.corner_radius > 0 {
            native_frame.apply(
                window.as_ref(),
                options.corner_radius,
                [surface_width, surface_height],
            );
        }
        // MyGO makes a window ready only after its first content frame. Metal
        // cannot acquire a hidden drawable, so wgpu first fills its retained
        // target and exposes the window only for the final surface copy.
        let deferred_show = renderer.defers_initial_show();
        if !deferred_show {
            window.set_visible(true);
        }

        Ok(Shell {
            #[cfg(target_os = "macos")]
            frame_clock: None,
            window,
            corner_radius,
            deferred_show,
            #[cfg(target_os = "macos")]
            native_frame,
            #[cfg(target_os = "macos")]
            native_input: Default::default(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            preferences: Default::default(),
            presentation_error: None,
            renderer,
            surface_size: [surface_width, surface_height],
            scene: Scene::new(),
            tcx,
            accessibility,
        })
    }

    pub(crate) fn start_window_drag(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(action) = self
            .native_input
            .prepare_titlebar_double_click(self.window.as_ref())
        {
            if action.minimizes() {
                self.prepare_hide();
            }
            action.perform();
            return true;
        }
        match self.window.drag_window() {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(?error, "native window manager rejected window drag");
                false
            }
        }
    }

    pub(crate) fn multi_click_settings(&self) -> plico_renderer::MultiClickSettings {
        #[cfg(target_os = "macos")]
        {
            crate::macos_input::multi_click_settings()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Default::default()
        }
    }

    pub(crate) fn display_preferences(&self) -> plico_renderer::DisplayPreferences {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            self.preferences.snapshot()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Default::default()
        }
    }
    pub(crate) fn observe_native_events(&mut self, _wake: crate::WakeCallback) {
        self.renderer.set_surface_wake(_wake.clone());
        #[cfg(target_os = "macos")]
        {
            self.frame_clock = crate::macos_frame_clock::NativeFrameClock::new(
                self.window.as_ref(),
                _wake.clone(),
            );
            self.preferences.observe(_wake);
        }
        #[cfg(target_os = "linux")]
        self.preferences.observe(_wake);
    }
    pub(crate) fn animation_frame_ready(&self, _continuous: bool) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(clock) = &self.frame_clock {
            return clock.frame_ready(_continuous);
        }
        true
    }
    pub(crate) fn presentation_deferred(&self) -> bool {
        self.renderer.presentation_deferred()
    }
    pub(crate) fn presentation_waits_for_display(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.frame_clock.is_some() && self.renderer.presentation_deferred()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
    pub(crate) fn presentation_deadline(&self) -> Option<std::time::Instant> {
        self.renderer.presentation_deadline()
    }
    pub(crate) fn take_surface_ready(&self) -> bool {
        self.renderer.take_surface_ready()
    }
    pub(crate) fn refresh_display_preferences(&mut self) -> bool {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            self.preferences.refresh()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    /// Current surface size (w,h).
    pub fn size(&self) -> (u32, u32) {
        (self.surface_size[0], self.surface_size[1])
    }

    /// Resize the physical render surface; zero-sized requests are ignored.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.resize_surface(width, height) {
            self.window.request_redraw();
        }
    }
    fn resize_surface(&mut self, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 || self.surface_size == [width, height] {
            return false;
        }
        self.surface_size = [width, height];
        self.renderer.resize(width, height);
        true
    }

    /// Synchronize native extent before building scene/IME geometry, including
    /// DPI events whose queued SurfaceResized notification has not arrived yet.
    pub(crate) fn sync_surface_size(&mut self) -> bool {
        let size = self.window.surface_size();
        // A frame/scale handler is already going to paint this geometry; do
        // not enqueue another redraw just for discovering the current extent.
        self.resize_surface(size.width, size.height)
    }
    /// Current physical-pixels-per-logical-pixel scale reported by the window.
    pub fn scale_factor(&self) -> f64 {
        self.window.scale_factor().max(f64::EPSILON)
    }

    /// Query maximize state without changing native window styles.
    pub fn is_maximized(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::{NSView, NSWindowStyleMask};
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};

            // winit 0.31 beta's is_maximized temporarily adds a title bar to
            // borderless/non-resizable windows. That emits resize events; when
            // queried while publishing metrics it creates an endless event loop.
            let Ok(handle) = self.window.window_handle() else {
                return false;
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return false;
            };
            // SAFETY: Shell owns the live window, and metrics are queried on
            // the AppKit event-loop thread. The borrowed NSView cannot outlive it.
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            let Some(window) = view.window() else {
                return false;
            };
            let required = NSWindowStyleMask::Titled | NSWindowStyleMask::Resizable;
            if window.styleMask().contains(required) {
                return window.isZoomed();
            }
            // AppKit's isZoomed is unreliable for borderless windows. Compare
            // their frame with the screen's work area, in logical coordinates.
            let Some(screen) = window.screen() else {
                return false;
            };
            let frame = window.frame();
            let work = screen.visibleFrame();
            return (frame.origin.x - work.origin.x).abs() <= 1.0
                && (frame.origin.y - work.origin.y).abs() <= 1.0
                && (frame.size.width - work.size.width).abs() <= 1.0
                && (frame.size.height - work.size.height).abs() <= 1.0;
        }
        #[cfg(not(target_os = "macos"))]
        self.window.is_maximized()
    }

    /// Set or clear borderless fullscreen on the primary monitor.
    ///
    /// Winit exposes exclusive video modes too; Plico deliberately uses the
    /// borderless variant so desktop shortcuts and multi-monitor movement keep
    /// working. Platforms may ignore the request while the window is not yet
    /// visible.
    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.window
            .set_fullscreen(fullscreen.then_some(winit::monitor::Fullscreen::Borderless(None)));
    }

    /// CSS/layout viewport in logical pixels; the surface remains physical.
    /// Current logical client-area size, clamped to at least 1×1.
    pub fn logical_size(&self) -> (u32, u32) {
        let (width, height) = self.size();
        let scale = self.scale_factor();
        (
            (width as f64 / scale).ceil().max(1.0) as u32,
            (height as f64 / scale).ceil().max(1.0) as u32,
        )
    }

    /// Apply client-area rounding after scene/HUD assembly, before screenshots.
    pub fn prepare_window_frame(&mut self, base_color: Color) -> Color {
        let radius = if self.is_maximized() {
            0
        } else {
            self.corner_radius
        };
        #[cfg(target_os = "macos")]
        if self.corner_radius > 0
            && self
                .native_frame
                .apply(self.window.as_ref(), radius, self.surface_size)
        {
            // Native continuous corners clip content and compositor shadow.
            // Preserve the scene clear; a second GPU clip would square off the
            // continuous contour and introduce another antialiasing fringe.
            return base_color;
        }
        let radius = f64::from(radius) * self.scale_factor();
        crate::window_frame::clip_window_frame(
            &mut self.scene,
            self.surface_size,
            radius,
            base_color,
        )
    }

    /// Accept [`Self::scene`], returning true for presentation or an unchanged-frame skip.
    pub fn present(&mut self, base_color: Color) -> bool {
        #[cfg(target_os = "macos")]
        {
            if let Some(clock) = &self.frame_clock {
                self.renderer
                    .set_presentation_clock(Some(clock.tick_sample()), clock.refresh_interval());
                clock.consume();
            } else {
                self.renderer
                    .set_presentation_clock(None, std::time::Duration::from_secs_f64(1. / 60.));
            }
        }
        self.accessibility.publish_root(self.window.as_ref());
        let result = if self.deferred_show {
            self.renderer.render_initial(
                &mut self.scene,
                self.surface_size[0],
                self.surface_size[1],
                base_color,
            )
        } else {
            self.renderer.render(
                &mut self.scene,
                self.surface_size[0],
                self.surface_size[1],
                base_color,
            )
        };
        if self.deferred_show && self.window.is_visible() == Some(true) {
            self.deferred_show = false;
        }
        match result {
            Ok(presented) => {
                self.presentation_error = None;
                presented
            }
            Err(error) => {
                let message = error.to_string();
                if self.presentation_error.as_deref() != Some(message.as_str()) {
                    crate::telemetry::report_error("window presentation failed", &error);
                    self.presentation_error = Some(message);
                }
                false
            }
        }
    }

    pub(crate) fn initial_show_pending(&self) -> bool {
        self.deferred_show
    }
    pub(crate) fn initial_show_error(&self) -> Option<String> {
        self.deferred_show
            .then(|| self.presentation_error.clone())
            .flatten()
    }

    pub(crate) fn damage_coverage(&self) -> Option<f64> {
        self.renderer
            .damage_coverage(self.surface_size[0], self.surface_size[1])
    }

    pub(crate) fn surface_acquire_pending(&self) -> bool {
        self.renderer.surface_acquire_pending()
    }
    pub(crate) fn present_skipped(&self) -> bool {
        self.renderer.present_skipped()
    }
    pub(crate) fn invalidate_presentation(&mut self) {
        self.renderer.invalidate_presentation();
    }
    pub(crate) fn maintain(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        self.renderer.maintain(now)
    }
    pub(crate) fn prepare_hide(&mut self) {
        self.animation_frame_ready(false);
        self.renderer.prepare_hide();
    }
    pub(crate) fn trim_gpu_cache(&mut self) {
        self.renderer.trim_gpu_cache();
    }

    /// Borrow the platform window.
    pub fn window(&self) -> &Arc<dyn Window> {
        &self.window
    }
    /// Mutably borrow shared text shaping resources.
    pub fn tcx_mut(&mut self) -> &mut TextContext {
        &mut self.tcx
    }
    /// Mutably borrow the frame's retained Wabou paint scene.
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
}
