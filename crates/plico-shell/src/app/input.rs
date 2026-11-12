use super::*;

impl App {
    pub(super) fn logical_pointer_position(&self, position: PhysicalPosition<f64>) -> Point {
        let scale = self
            .state
            .as_ref()
            .map_or(1.0, |shell| shell.scale_factor());
        Point {
            x: position.x / scale,
            y: position.y / scale,
        }
    }

    pub(super) fn update_pointer_state(
        &mut self,
        position: Point,
        buttons: u32,
        properties: PointerProperties,
    ) {
        self.pointer_states
            .insert(properties.id, (position, buttons, properties));
        if properties.primary {
            self.pointer_position = position;
            self.pointer_buttons = buttons;
        }
    }

    pub(super) fn handle_pointer_moved(
        &mut self,
        position: PhysicalPosition<f64>,
        properties: PointerProperties,
    ) {
        let position = self.logical_pointer_position(position);
        let buttons = self
            .pointer_states
            .get(&properties.id)
            .map_or(0, |state| state.1);
        self.update_pointer_state(position, buttons, properties);
        self.dispatch_event(UiEvent::Pointer(PointerEvent {
            phase: PointerPhase::Move,
            position,
            button: None,
            buttons,
            modifiers: self.modifiers,
            properties,
        }));
        self.sync_pointer_cursor();
    }

    pub(super) fn handle_pointer_entered(
        &mut self,
        position: PhysicalPosition<f64>,
        properties: PointerProperties,
    ) {
        let position = self.logical_pointer_position(position);
        let buttons = self
            .pointer_states
            .get(&properties.id)
            .map_or(0, |state| state.1);
        self.update_pointer_state(position, buttons, properties);
        self.dispatch_event(UiEvent::Pointer(PointerEvent {
            phase: PointerPhase::Enter,
            position,
            button: None,
            buttons,
            modifiers: self.modifiers,
            properties,
        }));
        self.sync_pointer_cursor();
    }

    pub(super) fn handle_pointer_left(
        &mut self,
        position: Option<PhysicalPosition<f64>>,
        properties: PointerProperties,
    ) {
        let old = self.pointer_states.get(&properties.id).copied();
        let position = position
            .map(|position| self.logical_pointer_position(position))
            .or_else(|| old.map(|state| state.0))
            .unwrap_or(self.pointer_position);
        let buttons = old.map_or(0, |state| state.1);
        self.update_pointer_state(position, buttons, properties);
        self.dispatch_event(UiEvent::Pointer(PointerEvent {
            phase: PointerPhase::Leave,
            position,
            button: None,
            buttons,
            modifiers: self.modifiers,
            properties,
        }));
        self.pointer_states.remove(&properties.id);
        self.sync_pointer_cursor();
    }

    pub(super) fn handle_pointer_button(
        &mut self,
        position: PhysicalPosition<f64>,
        source: ButtonSource,
        state: ElementState,
        properties: PointerProperties,
    ) {
        if let Some(key) = pointer_navigation_key(&source) {
            self.dispatch_event(UiEvent::Key(KeyEvent::logical(
                if state == ElementState::Pressed {
                    KeyPhase::Down
                } else {
                    KeyPhase::Up
                },
                key,
                self.modifiers,
            )));
            return;
        }
        if state == ElementState::Pressed {
            self.sync_multi_click_settings();
        }
        let button = pointer_button(&source);
        let position = self.logical_pointer_position(position);
        let mut buttons = self
            .pointer_states
            .get(&properties.id)
            .map_or(0, |state| state.1);
        self.update_pointer_state(position, buttons, properties);
        if state == ElementState::Pressed
            && properties.primary
            && button == PointerButton::Primary
            && let Some(direction) = self.custom_resize_direction()
            && let Some(shell) = self.state.as_ref()
        {
            match shell.window().drag_resize_window(direction) {
                Ok(()) => {
                    // The compositor owns the gesture until release. Do not
                    // also dispatch it into the UI tree or leave a pressed
                    // button bit set.
                    self.pointer_buttons = 0;
                    self.pointer_states.remove(&properties.id);
                    return;
                }
                Err(error) => {
                    tracing::warn!(
                        ?direction,
                        ?error,
                        "native window manager rejected custom window resize"
                    );
                }
            }
        }
        #[cfg(target_os = "linux")]
        if state == ElementState::Pressed
            && properties.pointer_type == plico_renderer::PointerType::Mouse
        {
            self.reset_ime_before_pointer_selection();
        }
        let phase = match state {
            ElementState::Pressed => {
                buttons |= button_mask(button);
                PointerPhase::Down
            }
            ElementState::Released => {
                buttons &= !button_mask(button);
                PointerPhase::Up
            }
        };
        self.update_pointer_state(position, buttons, properties);
        self.dispatch_event(UiEvent::Pointer(PointerEvent {
            phase,
            position,
            button: Some(button),
            buttons,
            modifiers: self.modifiers,
            properties,
        }));
    }

    pub(super) fn handle_wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        let (delta_x, delta_y, delta_mode) = match delta {
            // Winit reports the direction content should move; the DOM/Wabou
            // event reports the scroll-position delta.
            MouseScrollDelta::LineDelta(x, y) => (
                -f64::from(x) * crate::WHEEL_LINE_DELTA,
                -f64::from(y) * crate::WHEEL_LINE_DELTA,
                WheelDeltaMode::Line,
            ),
            MouseScrollDelta::PixelDelta(position) => {
                let scale = self
                    .state
                    .as_ref()
                    .map_or(1.0, |shell| shell.scale_factor());
                (
                    -position.x / scale,
                    -position.y / scale,
                    WheelDeltaMode::Pixel,
                )
            }
            // Non-exhaustive upstream: unknown scroll units degrade to pixels.
            _ => (0., 0., WheelDeltaMode::Pixel),
        };
        // Winit's wheel event has no position, so expose the latest logical
        // pointer position as a stable framework invariant.
        self.dispatch_event(UiEvent::Wheel(WheelEvent {
            position: self.pointer_position,
            delta_x,
            delta_y,
            delta_mode,
            phase: gesture_phase(phase),
            modifiers: self.modifiers,
        }));
    }

    /// Drain work which does not require a native surface. This remains active
    /// for close-to-tray windows whose frame source is intentionally retained.
    pub(super) fn poll_background_work(&mut self) -> bool {
        let preferences_changed = self
            .state
            .as_mut()
            .is_some_and(|shell| shell.refresh_display_preferences());
        if preferences_changed {
            self.sync_window_metrics();
        }
        let surface_ready = self
            .state
            .as_ref()
            .is_some_and(|shell| shell.take_surface_ready());
        preferences_changed
            | surface_ready
            | self.poll_modal_effects()
            | self.poll_effect_completions()
            | self.source.poll_async()
            | self.drain_host_actions()
            | self.drain_effects()
    }
}
