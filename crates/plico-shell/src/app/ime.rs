use super::*;

impl App {
    pub(super) fn observe_ime_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::Ime(ImeEvent::Preedit { text, .. }) => {
                self.ime_composing = !text.is_empty();
            }
            UiEvent::Ime(ImeEvent::Commit(_)) | UiEvent::Ime(ImeEvent::Disabled) => {
                self.ime_composing = false;
            }
            _ => {}
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn reset_ime_before_pointer_selection(&mut self) {
        if !self.ime_composing {
            return;
        }

        // Wayland text-input-v3 has no reset request. We solve the same
        // problem by cycling the native text-input session before delivering
        // the pointer press. Clear the widget preedit first so the old
        // composition cannot move with the new caret or leak into a newly
        // focused editor.
        self.dispatch_event(UiEvent::Ime(ImeEvent::Preedit {
            text: String::new(),
            cursor: None,
        }));

        if !self.ime_enabled {
            return;
        }
        let Some(shell) = self.state.as_ref() else {
            return;
        };
        match shell.window().request_ime_update(ImeRequest::Disable) {
            Ok(()) => {
                self.ime_enabled = false;
                self.enable_ime_if_ready();
            }
            Err(error) => {
                tracing::warn!(
                    target: "plico::ime",
                    %error,
                    "failed to reset IME before pointer selection"
                );
            }
        }
    }

    pub(super) fn enable_ime_if_ready(&mut self) {
        if !self.ime_requested || self.ime_enabled {
            return;
        }
        let Some(shell) = self.state.as_ref() else {
            return;
        };
        // Enable composition immediately when focus enters an editor. Waiting
        // for a painted caret makes AppKit begin the first preedit against
        // stale geometry; the complete snapshot is published after layout.
        let capabilities = ImeCapabilities::new()
            .with_cursor_area()
            .with_hint_and_purpose()
            .with_surrounding_text();
        let (mut position, size) =
            ime_cursor_rect(self.ime_state.as_ref().map(|state| state.cursor_area));
        let [offset_x, offset_y] = shell.content_offset();
        position.x += offset_x;
        position.y += offset_y;
        let surrounding = self
            .ime_state
            .as_ref()
            .and_then(|state| {
                ImeSurroundingText::new(
                    state.surrounding_text.clone(),
                    state.surrounding_cursor,
                    state.surrounding_anchor,
                )
                .ok()
            })
            .unwrap_or_else(|| ImeSurroundingText::new(String::new(), 0, 0).unwrap());
        let (hint, purpose) = ime_hint_and_purpose(self.ime_state.as_ref());
        let data = ImeRequestData::default()
            .with_cursor_area(position.into(), size.into())
            .with_hint_and_purpose(hint, purpose)
            .with_surrounding_text(surrounding);
        let request = ImeRequest::Enable(
            ImeEnableRequest::new(capabilities, data)
                .expect("IME capabilities and initial data must match"),
        );
        if shell.window().request_ime_update(request).is_ok() {
            self.ime_enabled = true;
            tracing::debug!(target: "plico::ime", "enabled IME for focused editor");
        }
    }

    pub(super) fn update_ime_state(&mut self) {
        let state = self.source.ime_state();
        // A declarative autofocus can establish a text client before any key or
        // pointer event has supplied EventResponse::text_input.
        if state.is_some() {
            self.ime_requested = true;
        } else if self.ime_state.is_some() {
            self.ime_requested = false;
            if self.ime_enabled
                && let Some(shell) = self.state.as_ref()
                && shell
                    .window()
                    .request_ime_update(ImeRequest::Disable)
                    .is_ok()
            {
                self.ime_enabled = false;
            }
        }
        let changed = state != self.ime_state;
        let restart = self.ime_enabled
            && self.ime_requested
            && ime_client_changed(self.ime_state.as_ref(), state.as_ref());
        self.ime_state = state;
        if restart && let Some(shell) = self.state.as_ref() {
            match shell.window().request_ime_update(ImeRequest::Disable) {
                Ok(()) => {
                    self.ime_enabled = false;
                    tracing::debug!(
                        target: "plico::ime",
                        "restarting IME after focused text client changed"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        target: "plico::ime",
                        %error,
                        "failed to reset IME after focused text client changed"
                    );
                }
            }
        }
        if self.ime_requested && !self.ime_enabled {
            self.enable_ime_if_ready();
        }
        if !changed {
            return;
        }
        if !self.ime_enabled {
            return;
        }
        if let Some(shell) = self.state.as_ref() {
            let (mut position, size) =
                ime_cursor_rect(self.ime_state.as_ref().map(|state| state.cursor_area));
            let [offset_x, offset_y] = shell.content_offset();
            position.x += offset_x;
            position.y += offset_y;
            let mut data = ImeRequestData::default().with_cursor_area(position.into(), size.into());
            let (hint, purpose) = ime_hint_and_purpose(self.ime_state.as_ref());
            data = data.with_hint_and_purpose(hint, purpose);
            if let Some(state) = self.ime_state.as_ref()
                && let Ok(surrounding) = ImeSurroundingText::new(
                    state.surrounding_text.clone(),
                    state.surrounding_cursor,
                    state.surrounding_anchor,
                )
            {
                data = data.with_surrounding_text(surrounding);
            }
            if shell
                .window()
                .request_ime_update(ImeRequest::Update(data))
                .is_ok()
            {
                tracing::debug!(
                    target: "plico::ime",
                    x = position.x,
                    y = position.y,
                    width = size.width,
                    height = size.height,
                    "updated IME caret area"
                );
            }
        }
    }
}
