use super::*;

impl App {
    pub(super) fn custom_resize_direction(&self) -> Option<ResizeDirection> {
        (self.window_options.resizable
            && !self.window_options.decorations
            && !self.window_metrics.maximized)
            .then(|| {
                let shell = self.state.as_ref()?;
                let [offset_x, offset_y] = shell.content_offset();
                let (width, height) = shell.surface_logical_size();
                resize_direction_at(
                    Point {
                        x: self.pointer_position.x + offset_x,
                        y: self.pointer_position.y + offset_y,
                    },
                    f64::from(width),
                    f64::from(height),
                    CUSTOM_DECORATION_RESIZE_BORDER,
                    self.window_options.effective_resize_edges(),
                )
            })?
    }

    pub(super) fn sync_pointer_cursor(&self) {
        let Some(shell) = self.state.as_ref() else {
            return;
        };
        let cursor = self.custom_resize_direction().map_or_else(
            || match self.source.pointer_cursor() {
                CursorStyle::Default => CursorIcon::Default,
                CursorStyle::Pointer => CursorIcon::Pointer,
                CursorStyle::Text => CursorIcon::Text,
                CursorStyle::Crosshair => CursorIcon::Crosshair,
                CursorStyle::Move => CursorIcon::Move,
                CursorStyle::Wait => CursorIcon::Wait,
                CursorStyle::NotAllowed => CursorIcon::NotAllowed,
                CursorStyle::EwResize => CursorIcon::EwResize,
                CursorStyle::NsResize => CursorIcon::NsResize,
            },
            CursorIcon::from,
        );
        shell.window().set_cursor(Cursor::Icon(cursor));
    }
    pub(super) fn dispatch_focus_change(&mut self, focused: bool) {
        if !focused {
            let active = self
                .pointer_states
                .drain()
                .filter_map(|(_, state)| (state.1 != 0).then_some(state))
                .collect::<Vec<_>>();
            self.pointer_buttons = 0;
            for (position, _, properties) in active {
                self.dispatch_event(UiEvent::Pointer(PointerEvent {
                    phase: PointerPhase::Cancel,
                    position,
                    button: None,
                    buttons: 0,
                    modifiers: self.modifiers,
                    properties,
                }));
            }
            // Some platforms do not send matching key/modifier releases after
            // deactivation. Never carry those physical states into the next
            // focus session.
            self.set_modifiers(Modifiers::default());
        }
        self.dispatch_event(UiEvent::Focus(focused));
    }

    pub(super) fn set_modifiers(&mut self, modifiers: Modifiers) {
        if self.modifiers == modifiers {
            return;
        }
        self.modifiers = modifiers;
        self.dispatch_event(UiEvent::ModifiersChanged(modifiers));
    }

    /// Construct a single-window application using default window options.
    pub fn new(source: Box<dyn FrameSource>) -> Self {
        Self::with_options(source, WindowOptions::default())
    }

    /// Construct a single-window application with an initial logical size.
    pub fn with_size(source: Box<dyn FrameSource>, width: u32, height: u32) -> Self {
        Self::with_options(
            source,
            WindowOptions::new().initial_inner_size(width, height),
        )
    }

    /// Construct a single-window application with explicit native options.
    pub fn with_options(source: Box<dyn FrameSource>, window_options: WindowOptions) -> Self {
        let window_options = window_options.normalized();
        let (effect_completion_tx, effect_completion_rx) = std::sync::mpsc::channel();
        Self {
            window_key: crate::initial_window_resource_key(0),
            source,
            state: None,
            window_options,
            window_metrics: WindowMetrics::default(),
            modifiers: Modifiers::default(),
            pointer_buttons: 0,
            pointer_position: Point { x: 0.0, y: 0.0 },
            pointer_states: HashMap::new(),
            ime_requested: false,
            ime_enabled: false,
            file_drags: HashMap::new(),
            ime_composing: false,
            ime_state: None,
            startup_error: Arc::new(Mutex::new(None)),
            frame_stats: FrameStats::default(),
            present_retry_pending: false,
            clipboard: None,
            pending_windows: Vec::new(),
            pending_window_commands: Vec::new(),
            pending_extension_effects: Vec::new(),
            pending_modal_effects: FuturesUnordered::new(),
            effect_completion_tx,
            effect_completion_rx,
            wake_callback: None,
            close_requested: false,
            application_exit_requested: false,
            application_relaunch_requested: false,
            lifecycle: WindowLifecycle::visible(),
            force_semantics: false,
            scheduled_frame_deadline: None,
            skipped_present_deadline: None,
            multi_click_settings: None,
            performance_hud: crate::performance_hud::PerformanceHud::from_environment(),
        }
    }

    pub(super) fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.source.set_wake_callback(wake.clone());
        self.wake_callback = Some(wake);
    }

    pub(super) fn sync_multi_click_settings(&mut self) {
        if let Some(shell) = &self.state {
            let settings = shell.multi_click_settings();
            if self.multi_click_settings != Some(settings) {
                self.source.set_multi_click_settings(settings);
                self.multi_click_settings = Some(settings);
            }
        }
    }

    pub(super) fn sync_window_metrics(&mut self) {
        let Some(shell) = self.state.as_ref() else {
            return;
        };
        let (surface_width, surface_height) = shell.size();
        let (logical_width, logical_height) = shell.logical_size();
        let inset = (shell.client_frame_inset() * shell.scale_factor()).round() as u32;
        let physical_width = surface_width.saturating_sub(inset.saturating_mul(2)).max(1);
        let physical_height = surface_height
            .saturating_sub(inset.saturating_mul(2))
            .max(1);
        let outer_position = shell.window().outer_position().ok();
        let next = WindowMetrics {
            window_key: self.window_key,
            logical_width,
            logical_height,
            physical_width,
            physical_height,
            scale_factor: shell.scale_factor(),
            maximized: shell.is_maximized(),
            fullscreen: shell.window().fullscreen().is_some(),
            focused: shell.window().has_focus(),
            outer_x: outer_position.map(|position| position.x),
            outer_y: outer_position.map(|position| position.y),
            occluded: self.window_metrics.occluded,
            color_scheme: shell.window().theme().map(|theme| match theme {
                winit::window::Theme::Light => crate::ColorScheme::Light,
                winit::window::Theme::Dark => crate::ColorScheme::Dark,
            }),
            reduced_motion: shell.display_preferences().reduce_motion,
            reduced_transparency: shell.display_preferences().reduce_transparency,
            high_contrast: shell.display_preferences().high_contrast,
        };
        if next != self.window_metrics {
            self.window_metrics = next;
            self.dispatch_event(UiEvent::WindowMetrics(next));
        }
    }

    /// Build one frame into `shell.scene` from the source. Returns
    /// `(node_count, build_frame_ms, scene_ms)` so the caller can fold the
    /// timings (plus its own present timing) into the EMA perf stats.
    pub(super) fn build(
        shell: &mut Shell,
        source: &mut dyn FrameSource,
        parent: u64,
    ) -> crate::Result<(usize, f64, f64)> {
        shell.renderer.configure_text(&mut shell.tcx)?;
        let (w, h) = shell.logical_size();
        let scale = shell.scale_factor();
        source.set_device_scale(scale);
        let retained = {
            #[cfg(feature = "profiling")]
            let span = tracing::trace_span!(target: "plico::perf", "frame.retained");
            #[cfg(feature = "profiling")]
            let _guard = span.enter();
            let mut phase = crate::telemetry::span("frame", "retained.cpu");
            phase.attribute("parent", parent);
            source
                .render_retained_frame(&mut shell.scene, &mut shell.tcx, (w, h), scale)
                .map_err(|source| crate::Error::Frame { source })?
        };
        if let Some(timings) = retained {
            return match shell.tcx.take_native_paragraph_error() {
                None => Ok(timings),
                Some(message) => Err(crate::Error::Renderer { message }),
            };
        }
        let (node_count, build_frame_ms, scene_ms) = {
            #[cfg(feature = "profiling")]
            let span = tracing::trace_span!(target: "plico::perf", "frame.build");
            #[cfg(feature = "profiling")]
            let _guard = span.enter();
            let mut phase = crate::telemetry::span("frame", "build.cpu");
            phase.attribute("parent", parent);
            source
                .build_and_paint(&mut shell.scene, &mut shell.tcx, w, h, scale)
                .map_err(|source| crate::Error::Frame { source })?
        };
        if let Some(message) = shell.tcx.take_native_paragraph_error() {
            return Err(crate::Error::Renderer { message });
        }
        Ok((node_count, build_frame_ms, scene_ms))
    }
}
