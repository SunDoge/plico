use super::*;

impl App {
    pub(super) fn handle_window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) -> bool {
        let mut geometry_changed = false;
        if let Some(shell) = self.state.as_mut() {
            shell
                .accessibility
                .process_window_event(shell.window.as_ref(), &event);
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.source.close_requested() {
                    if let Some(shell) = self.state.as_ref() {
                        shell.window().request_redraw();
                    }
                    return false;
                }
                self.state = None;
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::SurfaceResized(PhysicalSize { width, height }) => {
                let Some(shell) = self.state.as_mut() else {
                    return false;
                };
                // Queued events can describe intermediate extents. Configure
                // only the latest native size and avoid a redundant redraw.
                if width == 0 || height == 0 {
                    return false;
                }
                geometry_changed = shell.sync_surface_size();
                self.present_retry_pending = false;
                self.sync_window_metrics();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(shell) = self.state.as_mut() {
                    geometry_changed = shell.sync_surface_size()
                        || shell.scale_factor() != self.window_metrics.scale_factor;
                }
                self.present_retry_pending = false;
                self.sync_window_metrics();
            }
            WindowEvent::ThemeChanged(_) => {
                self.sync_window_metrics();
                if let Some(shell) = self.state.as_ref() {
                    shell.window().request_redraw();
                }
            }
            WindowEvent::Moved(_) => self.sync_window_metrics(),
            WindowEvent::Occluded(occluded) => {
                if occluded
                    && !self.window_metrics.occluded
                    && let Some(shell) = self.state.as_mut()
                {
                    shell.trim_gpu_cache();
                }
                self.window_metrics.occluded = occluded;
                self.sync_window_metrics();
                if !occluded {
                    if let Some(shell) = self.state.as_mut() {
                        shell.invalidate_presentation();
                    }
                    self.present_retry_pending = false;
                }
                if !occluded && let Some(shell) = self.state.as_ref() {
                    shell.window().request_redraw();
                }
            }
            WindowEvent::PointerMoved {
                device_id,
                position,
                primary,
                source,
            } => self.handle_pointer_moved(
                position,
                pointer_source_properties(device_id, primary, &source),
            ),
            WindowEvent::PointerEntered {
                device_id,
                position,
                primary,
                kind,
            } => self.handle_pointer_entered(
                position,
                pointer_kind_properties(device_id, primary, kind),
            ),
            WindowEvent::PointerLeft {
                device_id,
                position,
                primary,
                kind,
            } => self
                .handle_pointer_left(position, pointer_kind_properties(device_id, primary, kind)),
            WindowEvent::PointerButton {
                device_id,
                position,
                primary,
                button,
                state,
                // Activation clicks are folded into regular pointer presses.
                is_macos_activation_click: _,
            } => {
                let properties = button_source_properties(device_id, primary, &button);
                self.handle_pointer_button(position, button, state, properties);
            }
            WindowEvent::MouseWheel { delta, phase, .. } => self.handle_wheel(delta, phase),
            WindowEvent::PinchGesture { delta, phase, .. } => {
                self.dispatch_event(UiEvent::Gesture(GestureEvent::Pinch {
                    delta,
                    phase: gesture_phase(phase),
                }));
            }
            WindowEvent::PanGesture { delta, phase, .. } => {
                let scale = self
                    .state
                    .as_ref()
                    .map_or(1.0, |shell| shell.scale_factor());
                self.dispatch_event(UiEvent::Gesture(GestureEvent::Pan {
                    delta_x: f64::from(delta.x) / scale,
                    delta_y: f64::from(delta.y) / scale,
                    phase: gesture_phase(phase),
                }));
            }
            WindowEvent::RotationGesture { delta, phase, .. } => {
                self.dispatch_event(UiEvent::Gesture(GestureEvent::Rotation {
                    delta: f64::from(delta),
                    phase: gesture_phase(phase),
                }));
            }
            WindowEvent::DoubleTapGesture { .. } => {
                self.dispatch_event(UiEvent::Gesture(GestureEvent::DoubleTap));
            }
            WindowEvent::TouchpadPressure {
                pressure, stage, ..
            } => {
                self.dispatch_event(UiEvent::Gesture(GestureEvent::Pressure {
                    pressure: f64::from(pressure),
                    stage,
                }));
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                let phase = match event.state {
                    ElementState::Pressed => KeyPhase::Down,
                    ElementState::Released => KeyPhase::Up,
                };
                // Winit reports ordinary composed text on KeyboardInput. IME
                // composition uses the separate Ime::Commit path below.
                let text = (event.state == ElementState::Pressed)
                    .then(|| {
                        committed_key_text(
                            event.text.as_deref(),
                            event.text_with_all_modifiers.as_deref(),
                            self.modifiers,
                            self.ime_composing,
                        )
                    })
                    .flatten();
                let response = self.dispatch_event(UiEvent::Key(KeyEvent {
                    phase,
                    key: logical_key_name(&event.logical_key),
                    key_without_modifiers: logical_key_name(&event.key_without_modifiers),
                    code: format!("{:?}", event.physical_key),
                    text: event.text.as_deref().map(str::to_owned),
                    text_with_all_modifiers: event
                        .text_with_all_modifiers
                        .as_deref()
                        .map(str::to_owned),
                    location: key_location(event.location),
                    modifiers: self.modifiers,
                    repeat: event.repeat,
                    synthetic: is_synthetic,
                }));
                if let Some(text) = unconsumed_key_text(text, &response) {
                    self.dispatch_event(UiEvent::TextInput(text));
                }
            }
            WindowEvent::ModifiersChanged(state) => {
                self.set_modifiers(translate_modifiers(state.state()));
            }
            WindowEvent::Ime(event) => {
                let event = match event {
                    winit::event::Ime::Enabled => ImeEvent::Enabled,
                    winit::event::Ime::Preedit(text, cursor) => ImeEvent::Preedit { text, cursor },
                    winit::event::Ime::Commit(text) => ImeEvent::Commit(text),
                    winit::event::Ime::DeleteSurrounding {
                        before_bytes,
                        after_bytes,
                    } => ImeEvent::DeleteSurrounding {
                        before_bytes,
                        after_bytes,
                    },
                    winit::event::Ime::Disabled => ImeEvent::Disabled,
                    // `Ime` is non-exhaustive upstream; unknown IME lifecycle
                    // events carry no text and do not change composition state.
                    _ => return false,
                };
                self.dispatch_event(UiEvent::Ime(event));
            }
            WindowEvent::Focused(focused) => {
                self.dispatch_focus_change(focused);
                self.sync_window_metrics();
            }
            WindowEvent::DragEntered { id, position } => {
                let position = position.map(|p| self.logical_pointer_position(p));
                match event_loop.fetch_data_transfer(id, &TypeHint::UriList) {
                    Ok(serial) => {
                        self.file_drags.insert(
                            id,
                            IncomingFileDrag {
                                serial,
                                paths: None,
                                position,
                                dropped: false,
                            },
                        );
                        let _ = event_loop
                            .set_valid_dnd_actions(id, &[winit::event_loop::DndAction::Copy]);
                    }
                    Err(error) => tracing::warn!(%error, "could not request dropped file URIs"),
                }
                self.dispatch_event(UiEvent::FileDrop(FileDropEvent {
                    phase: FileDropPhase::Entered,
                    paths: Vec::new(),
                    position,
                }));
            }
            WindowEvent::DragPosition { id, position, .. } => {
                let position = self.logical_pointer_position(position);
                if let Some(drag) = self.file_drags.get_mut(&id) {
                    drag.position = Some(position);
                }
                self.dispatch_event(UiEvent::FileDrop(FileDropEvent {
                    phase: FileDropPhase::Moved,
                    paths: Vec::new(),
                    position: Some(position),
                }));
            }
            WindowEvent::DragLeft { id } => {
                if let Some(drag) = self.file_drags.remove(&id) {
                    self.dispatch_event(UiEvent::FileDrop(FileDropEvent {
                        phase: FileDropPhase::Left,
                        paths: Vec::new(),
                        position: drag.position,
                    }));
                }
            }
            WindowEvent::DragDropped { id, .. } => {
                let event = self.file_drags.get_mut(&id).and_then(|drag| {
                    drag.dropped = true;
                    drag.drop_event()
                });
                if let Some(event) = event {
                    self.file_drags.remove(&id);
                    self.dispatch_event(UiEvent::FileDrop(event));
                }
            }
            WindowEvent::DataTransferReceived { id, serial, value } => {
                let event = self
                    .file_drags
                    .get_mut(&id)
                    .filter(|drag| drag.serial == serial)
                    .and_then(|drag| {
                        // A failed/unsupported URI decode completes with no paths,
                        // rather than leaving a dropped transfer pending forever.
                        drag.paths = Some(
                            value
                                .try_as_uris()
                                .map(|uris| {
                                    uris.iter().filter_map(|uri| file_uri_path(uri)).collect()
                                })
                                .unwrap_or_default(),
                        );
                        drag.drop_event()
                    });
                if let Some(event) = event {
                    self.file_drags.remove(&id);
                    self.dispatch_event(UiEvent::FileDrop(event));
                }
            }
            _ => {}
        }
        geometry_changed
    }

    pub(super) fn next_frame_wake(&mut self, now: Instant) -> FrameWake {
        let maintenance = self.state.as_mut().and_then(|shell| shell.maintain(now));
        let source_animation = self.source.has_anim();
        let fallback = self.state.as_ref().and_then(Shell::presentation_deadline);
        let waits_for_tick = self
            .state
            .as_ref()
            .is_some_and(Shell::presentation_waits_for_display);
        let source_deadline = (!source_animation || fallback.is_some())
            .then(|| self.source.animation_deadline())
            .flatten();
        let (has_animation, deadline) =
            presentation_work(source_animation, source_deadline, fallback);
        let mut wake = scheduled_frame_wake(
            self.state.is_some()
                && self.lifecycle.presence() == WindowPresence::Visible
                && !self.window_metrics.occluded,
            has_animation,
            self.skipped_present_deadline,
            deadline,
            &mut self.scheduled_frame_deadline,
            now,
            self.state
                .as_ref()
                .is_some_and(Shell::surface_acquire_pending),
        );
        if let Some(shell) = &self.state {
            let waiting = waits_for_tick
                && self.lifecycle.presence() == WindowPresence::Visible
                && !self.window_metrics.occluded;
            let ready =
                shell.animation_frame_ready(waiting || has_animation && wake == FrameWake::Redraw);
            wake = display_wait_wake(wake, has_animation, waiting, ready);
        }
        wake.with_maintenance(maintenance)
    }

    pub(super) fn drain_proxy_wake_up(&mut self) -> bool {
        let changed = self.poll_background_work();
        if changed && let Some(shell) = self.state.as_ref() {
            shell.window().request_redraw();
        }
        changed
    }
}
