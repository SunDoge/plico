use super::*;

impl App {
    /// Open the platform clipboard on first use, returning it when available.
    pub(super) fn clipboard_mut(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            self.clipboard = match arboard::Clipboard::new() {
                Ok(clipboard) => Some(clipboard),
                Err(error) => {
                    tracing::warn!(%error,"cannot initialize system clipboard");
                    None
                }
            };
        }
        self.clipboard.as_mut()
    }

    pub(super) fn read_clipboard_text(&mut self) -> Option<String> {
        match self.clipboard_mut()?.get_text() {
            Ok(text) => Some(text),
            Err(error) => {
                tracing::warn!(%error,"cannot read clipboard text; no clipboard content logged");
                None
            }
        }
    }
    /// Run a modal effect (dialog) future to completion on the event loop.
    pub(super) fn push_modal_effect<F, T>(
        &mut self,
        id: crate::EffectId,
        op: crate::EffectOp,
        run: F,
    ) where
        F: FnOnce(Option<&dyn winit::window::Window>) -> T,
        T: Future<Output = crate::EffectResult> + 'static,
    {
        let parent = self.state.as_ref().map(|shell| &**shell.window());
        let future = run(parent);
        self.pending_modal_effects.push(Box::pin(async move {
            crate::EffectCompletion {
                id,
                op,
                result: future.await,
            }
        }));
    }

    pub(super) fn drain_host_actions(&mut self) -> bool {
        let mut handled = false;
        while let Some(action) = self.source.take_host_action() {
            handled = true;
            match action {
                HostAction::Window(command) => apply_window_command(self, command),
                // Native-widget backends emit completed clipboard writes.
                // Read requests with widget request IDs still belong to the
                // effect bridge and must not be silently dropped here.
                HostAction::SetClipboard(text) => {
                    if let Some(clipboard) = self.clipboard_mut() {
                        let _ = clipboard.set_text(text);
                    }
                }
                HostAction::ReadClipboard { .. } => {
                    tracing::error!(
                        action = ?action,
                        "clipboard widget action reached the shell untranslated"
                    );
                }
                HostAction::OpenUrl(url) => {
                    if let Some(url) = allowed_external_url(&url) {
                        let _ = open::that_detached(url.as_str());
                    } else {
                        tracing::warn!(url, "refused unsafe external URL");
                    }
                }
                HostAction::SetWindowTitle(title) => {
                    if let Some(shell) = self.state.as_ref() {
                        shell
                            .window()
                            .set_title(title.as_deref().unwrap_or(&self.window_options.title));
                    }
                }
                HostAction::RequestAttention => {
                    if let Some(shell) = self.state.as_ref() {
                        shell
                            .window()
                            .request_user_attention(Some(UserAttentionType::Informational));
                    }
                }
            }
        }
        handled
    }

    pub(super) fn drain_effects(&mut self) -> bool {
        let mut handled = false;
        while let Some(request) = self.source.take_effect() {
            handled = true;
            let id = request.id;
            let op = request.payload.op();
            match request.payload {
                crate::EffectPayload::ClipboardRead => {
                    let text = self.read_clipboard_text();
                    self.source.complete_effect(crate::EffectCompletion {
                        id,
                        op,
                        result: crate::EffectResult::ClipboardText(text),
                    });
                }
                crate::EffectPayload::ClipboardWrite { text } => {
                    let result = if self
                        .clipboard_mut()
                        .is_some_and(|clipboard| clipboard.set_text(text).is_ok())
                    {
                        crate::EffectResult::Unit
                    } else {
                        crate::EffectResult::Error {
                            code: crate::EffectErrorCode::PlatformFailure,
                            message: "native clipboard write failed".into(),
                        }
                    };
                    self.source
                        .complete_effect(crate::EffectCompletion { id, op, result });
                }
                crate::EffectPayload::WindowCreate(request) => {
                    self.pending_windows.push(PendingWindowEffect {
                        id,
                        op,
                        options: request.options,
                    });
                }
                crate::EffectPayload::WindowControl { window_id, command } => {
                    self.pending_window_commands.push((window_id, command));
                    self.source.complete_effect(crate::EffectCompletion {
                        id,
                        op,
                        result: crate::EffectResult::Unit,
                    });
                }
                crate::EffectPayload::AppDirsResolve(directories) => {
                    self.source.complete_effect(crate::EffectCompletion {
                        id,
                        op,
                        result: crate::EffectResult::AppDirectories(directories),
                    });
                }
                crate::EffectPayload::DialogOpen(request) => {
                    self.push_modal_effect(id, op, move |parent| {
                        let future = crate::system::open_dialog(parent, request);
                        async move { crate::EffectResult::DialogPaths(future.await) }
                    });
                }
                crate::EffectPayload::DialogSave(request) => {
                    self.push_modal_effect(id, op, move |parent| {
                        let future = crate::system::save_dialog(parent, request);
                        async move { crate::EffectResult::DialogPaths(future.await) }
                    });
                }
                crate::EffectPayload::DialogPickDirectory(request) => {
                    self.push_modal_effect(id, op, move |parent| {
                        let future = crate::system::pick_directory(parent, request);
                        async move { crate::EffectResult::DialogPaths(future.await) }
                    });
                }
                crate::EffectPayload::DialogMessage(request) => {
                    self.push_modal_effect(id, op, move |parent| {
                        let future = crate::system::message_dialog(parent, request);
                        async move { crate::EffectResult::DialogMessage(future.await) }
                    });
                }
                crate::EffectPayload::NotificationShow(request) => {
                    if request.title.trim().is_empty() {
                        self.source.complete_effect(crate::EffectCompletion {
                            id,
                            op,
                            result: crate::EffectResult::Error {
                                code: crate::EffectErrorCode::InvalidRequest,
                                message: "notification title must not be empty".into(),
                            },
                        });
                    } else {
                        let app_name = self.window_options.title.clone();
                        let completion_tx = self.effect_completion_tx.clone();
                        let wake = self.wake_callback.clone();
                        std::thread::spawn(move || {
                            let result = match crate::system::show_notification(&app_name, request)
                            {
                                Ok(()) => crate::EffectResult::Unit,
                                Err(message) => crate::EffectResult::Error {
                                    code: crate::EffectErrorCode::PlatformFailure,
                                    message,
                                },
                            };
                            if completion_tx
                                .send(crate::EffectCompletion { id, op, result })
                                .is_ok()
                                && let Some(wake) = wake
                            {
                                wake();
                            }
                        });
                    }
                }
                crate::EffectPayload::ApplicationExit => {
                    self.application_exit_requested = true;
                    self.source.complete_effect(crate::EffectCompletion {
                        id,
                        op,
                        result: crate::EffectResult::Unit,
                    });
                }
                crate::EffectPayload::ApplicationRelaunch => {
                    self.application_relaunch_requested = true;
                    self.source.complete_effect(crate::EffectCompletion {
                        id,
                        op,
                        result: crate::EffectResult::Unit,
                    });
                }
                payload @ (crate::EffectPayload::ContextMenuShow(_)
                | crate::EffectPayload::Extension { .. }) => {
                    self.pending_extension_effects.push(crate::EffectRequest {
                        id,
                        scope: request.scope,
                        payload,
                    });
                }
                crate::EffectPayload::Invalid { message, .. } => {
                    self.source.complete_effect(crate::EffectCompletion {
                        id,
                        op,
                        result: crate::EffectResult::Error {
                            code: crate::EffectErrorCode::InvalidRequest,
                            message,
                        },
                    });
                }
            }
        }
        handled
    }

    pub(super) fn poll_modal_effects(&mut self) -> bool {
        let Some(wake) = self.wake_callback.clone() else {
            return false;
        };
        let waker = Waker::from(Arc::new(CallbackWaker(wake)));
        let mut context = Context::from_waker(&waker);
        let mut changed = false;
        while let Poll::Ready(Some(completion)) =
            Pin::new(&mut self.pending_modal_effects).poll_next(&mut context)
        {
            changed = true;
            self.source.complete_effect(completion);
        }
        changed
    }

    pub(super) fn poll_effect_completions(&mut self) -> bool {
        let mut changed = false;
        while let Ok(completion) = self.effect_completion_rx.try_recv() {
            changed = true;
            self.source.complete_effect(completion);
        }
        changed
    }

    pub(super) fn fail_initial_frame(&mut self, error: crate::Error) {
        *self
            .startup_error
            .lock()
            .expect("startup error mutex poisoned") = Some(error);
        self.application_exit_requested = true;
    }
}
