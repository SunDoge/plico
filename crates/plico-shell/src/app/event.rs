use super::*;

impl App {
    pub(super) fn route_event(&mut self, event: UiEvent) -> EventResponse {
        // Renderer backends no longer intercept events; the source owns routing.
        self.source.handle_event(event)
    }
    pub(super) fn dispatch_event(&mut self, event: UiEvent) -> EventResponse {
        self.observe_ime_event(&event);
        if let Some(shell) = self.state.as_mut()
            && let Err(source) = self.source.prepare_for_event(&mut shell.tcx)
        {
            crate::telemetry::report_error(
                "event preparation rejected",
                &crate::Error::Frame { source },
            );
            return EventResponse::default();
        }
        let mut response = self.route_event(event);
        if let Some(request) = response.clipboard.take() {
            match request {
                ClipboardRequest::Write(text) => {
                    if let Some(clipboard) = self.clipboard_mut() {
                        let _ = clipboard.set_text(text);
                    }
                }
                ClipboardRequest::Read => {
                    let text = self.read_clipboard_text();
                    if let Some(text) = text.filter(|text| !text.is_empty()) {
                        let pasted = self.route_event(UiEvent::Paste(text));
                        response.handled |= pasted.handled;
                        response.request_redraw |= pasted.request_redraw;
                        response.consume_key_text |= pasted.consume_key_text;
                    }
                }
            }
        }
        let host_action = self.drain_host_actions();
        let effect =
            self.drain_effects() | self.poll_modal_effects() | self.poll_effect_completions();
        response.handled |= host_action || effect;
        response.request_redraw |= host_action || effect;
        if let Some(allowed) = response.text_input {
            self.ime_requested = allowed;
            if allowed {
                self.enable_ime_if_ready();
            } else if self.ime_enabled
                && let Some(shell) = self.state.as_ref()
                && shell
                    .window()
                    .request_ime_update(ImeRequest::Disable)
                    .is_ok()
            {
                self.ime_enabled = false;
            }
        }
        if response.request_redraw
            && let Some(shell) = self.state.as_ref()
        {
            shell.window().request_redraw();
        }
        response
    }
}
