use super::*;

impl MultiWindowApp {
    pub(super) fn new(
        mut apps: Vec<App>,
        factory: Option<FrameSourceFactory>,
        wake: WakeCallback,
        extensions: Vec<Box<dyn ShellExtension>>,
        relaunch_requested: Arc<AtomicBool>,
    ) -> Self {
        let force_semantics = extensions
            .iter()
            .any(|extension| extension.requires_semantics());
        for app in &mut apps {
            app.force_semantics = force_semantics;
        }
        let startup_errors = apps.iter().map(|app| app.startup_error.clone()).collect();
        Self {
            pending: apps,
            windows: HashMap::new(),
            hidden_windows: HashMap::new(),
            window_resources: SlotMap::with_key(),
            startup_errors,
            factory,
            wake,
            extensions,
            extensions_initialized: false,
            extensions_shutdown: false,
            extension_error: Arc::new(Mutex::new(None)),
            force_semantics,
            relaunch_requested,
        }
    }

    pub(super) fn extension_context<'a>(
        windows: &'a mut HashMap<WindowId, App>,
        hidden_windows: &'a mut HashMap<WindowResourceKey, App>,
        event_loop: &'a dyn ActiveEventLoop,
    ) -> ExtensionContext<'a> {
        ExtensionContext {
            windows,
            hidden_windows,
            event_loop,
        }
    }

    pub(super) fn release_window_resource(&mut self, window_key: WindowResourceKey) {
        let key = WindowSlotKey::from(KeyData::from_ffi(window_key.into_packed()));
        self.window_resources.remove(key);
    }

    /// Route every close request through extensions before releasing the
    /// logical window. Native decorations and custom titlebars must have the
    /// same close-to-tray and persistence semantics.
    pub(super) fn request_window_close(
        &mut self,
        window_key: WindowResourceKey,
        event_loop: &dyn ActiveEventLoop,
    ) {
        let prevented = self
            .windows
            .values_mut()
            .find(|app| app.window_key == window_key)
            .or_else(|| self.hidden_windows.get_mut(&window_key))
            .is_some_and(|app| {
                let prevented = app.source.close_requested();
                if prevented && let Some(shell) = app.state.as_ref() {
                    shell.window().request_redraw();
                }
                prevented
            });
        if prevented {
            return;
        }

        let mut context =
            Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
        if self
            .extensions
            .iter_mut()
            .any(|extension| extension.close_requested(window_key, &mut context))
        {
            return;
        }

        if let Some(window_id) = self
            .windows
            .iter()
            .find_map(|(id, app)| (app.window_key == window_key).then_some(*id))
        {
            self.windows.remove(&window_id);
            self.release_window_resource(window_key);
            return;
        }
        if self.hidden_windows.remove(&window_key).is_some() {
            self.release_window_resource(window_key);
        }
    }

    pub(super) fn poll_extensions(&mut self, event_loop: &dyn ActiveEventLoop) {
        let mut context =
            Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
        for extension in &mut self.extensions {
            extension.poll(&mut context);
        }
    }

    pub(super) fn apply_extension_effects(&mut self, event_loop: &dyn ActiveEventLoop) {
        let requests = self
            .windows
            .values_mut()
            .flat_map(|app| {
                let window_key = app.window_key;
                std::mem::take(&mut app.pending_extension_effects)
                    .into_iter()
                    .map(move |request| (window_key, request))
            })
            .collect::<Vec<_>>();
        for (window_key, request) in requests {
            let mut context =
                Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
            let handled = self
                .extensions
                .iter_mut()
                .any(|extension| extension.submit_effect(&request, &mut context));
            if !handled {
                context.complete_effect(
                    window_key,
                    crate::EffectCompletion {
                        id: request.id,
                        op: request.payload.op(),
                        result: crate::EffectResult::Error {
                            code: crate::EffectErrorCode::Unsupported,
                            message: format!(
                                "unsupported native effect {:?}",
                                request.payload.op()
                            ),
                        },
                    },
                );
            }
        }
    }

    pub(super) fn apply_window_requests(&mut self, event_loop: &dyn ActiveEventLoop) {
        let requests = self
            .windows
            .values_mut()
            .flat_map(|app| {
                let owner = app.window_key;
                std::mem::take(&mut app.pending_windows)
                    .into_iter()
                    .map(move |request| (owner, request))
            })
            .collect::<Vec<_>>();
        for (owner, request) in requests {
            let Some(factory) = self.factory.clone() else {
                if let Some(app) = find_window_by_key(self.windows.values_mut(), owner) {
                    app.source.complete_effect(crate::EffectCompletion {
                        id: request.id,
                        op: request.op,
                        result: crate::EffectResult::Error {
                            code: crate::EffectErrorCode::Unsupported,
                            message: "this host has no window source factory".into(),
                        },
                    });
                }
                continue;
            };
            let key = self.window_resources.insert(());
            let window_id = key.data().as_ffi();
            let window_key = crate::WindowResourceKey::from_packed(window_id)
                .expect("live window registry key must be valid");
            let result = match factory(window_key, &request.options) {
                Ok(source) => {
                    let mut app = App::with_options(source, request.options);
                    app.force_semantics = self.force_semantics;
                    app.set_wake_callback(self.wake.clone());
                    app.window_key = window_key;
                    app.can_create_surfaces(event_loop);
                    if let Some(id) = app.state.as_ref().map(|shell| shell.window().id()) {
                        self.startup_errors.push(app.startup_error.clone());
                        self.windows.insert(id, app);
                        crate::EffectResult::Window(window_key)
                    } else {
                        self.window_resources.remove(key);
                        crate::EffectResult::Error {
                            code: crate::EffectErrorCode::PlatformFailure,
                            message: "native window surface creation failed".into(),
                        }
                    }
                }
                Err(error) => {
                    self.window_resources.remove(key);
                    tracing::error!(window_id, %error, "failed to create window runtime");
                    crate::EffectResult::Error {
                        code: crate::EffectErrorCode::PlatformFailure,
                        message: error.to_string(),
                    }
                }
            };
            let completion = crate::EffectCompletion {
                id: request.id,
                op: request.op,
                result,
            };
            if let Some(app) = find_window_by_key(self.windows.values_mut(), owner) {
                app.source.complete_effect(completion);
            }
        }
        let commands = self
            .windows
            .values_mut()
            .chain(self.hidden_windows.values_mut())
            .flat_map(|app| std::mem::take(&mut app.pending_window_commands))
            .collect::<Vec<_>>();
        for (window_id, command) in commands {
            if matches!(command, WindowCommand::Show) {
                Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop)
                    .show_window(window_id);
                continue;
            }
            let Some(app) = find_window_by_key(self.windows.values_mut(), window_id)
                .or_else(|| find_window_by_key(self.hidden_windows.values_mut(), window_id))
            else {
                tracing::warn!(window_id = %window_id, "ignored command for unknown window");
                continue;
            };
            apply_window_command(app, command);
        }
        if self
            .windows
            .values()
            .chain(self.hidden_windows.values())
            .any(|app| app.application_exit_requested || app.application_relaunch_requested)
        {
            if self
                .windows
                .values()
                .chain(self.hidden_windows.values())
                .any(|app| app.application_relaunch_requested)
            {
                self.relaunch_requested.store(true, Ordering::Release);
            }
            self.shutdown_extensions(event_loop);
            event_loop.exit();
            return;
        }
        let close_requests = self
            .windows
            .values_mut()
            .filter_map(|app| {
                app.close_requested.then(|| {
                    app.close_requested = false;
                    app.window_key
                })
            })
            .collect::<Vec<_>>();
        for window_key in close_requests {
            self.request_window_close(window_key, event_loop);
        }
        if self.windows.is_empty() && self.hidden_windows.is_empty() {
            self.shutdown_extensions(event_loop);
            event_loop.exit();
        }
    }

    pub(super) fn shutdown_extensions(&mut self, event_loop: &dyn ActiveEventLoop) {
        if !self.extensions_initialized || self.extensions_shutdown {
            return;
        }
        self.extensions_shutdown = true;
        let mut context =
            Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
        for extension in self.extensions.iter_mut().rev() {
            extension.shutdown(&mut context);
        }
    }
}

impl ApplicationHandler for MultiWindowApp {
    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        for app in self
            .pending
            .iter_mut()
            .chain(self.windows.values_mut())
            .chain(self.hidden_windows.values_mut())
        {
            app.resumed(event_loop);
        }
    }

    fn suspended(&mut self, event_loop: &dyn ActiveEventLoop) {
        for app in self
            .pending
            .iter_mut()
            .chain(self.windows.values_mut())
            .chain(self.hidden_windows.values_mut())
        {
            app.suspended(event_loop);
        }
    }

    fn memory_warning(&mut self, event_loop: &dyn ActiveEventLoop) {
        for app in self
            .pending
            .iter_mut()
            .chain(self.windows.values_mut())
            .chain(self.hidden_windows.values_mut())
        {
            app.memory_warning(event_loop);
        }
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        for mut app in self.pending.drain(..) {
            app.window_key =
                WindowResourceKey::from_packed(self.window_resources.insert(()).data().as_ffi())
                    .expect("live window registry key must be valid");
            app.can_create_surfaces(event_loop);
            if let Some(id) = app.state.as_ref().map(|shell| shell.window().id()) {
                self.windows.insert(id, app);
            }
        }
        if !self.extensions_initialized {
            self.extensions_initialized = true;
            for extension in &mut self.extensions {
                if let Err(message) = extension.initialize(self.wake.clone()) {
                    *self
                        .extension_error
                        .lock()
                        .expect("extension error mutex poisoned") =
                        Some(crate::Error::Extension { message });
                    self.shutdown_extensions(event_loop);
                    event_loop.exit();
                    return;
                }
            }
        }
        // Extensions may have startup visibility/placement work even when the
        // application is idle and no service or animation emits a wake.
        self.poll_extensions(event_loop);
        // Initial render may synchronously call createWindow()/close(). The
        // wake callback is not installed until the source enters this event
        // loop, so explicitly drain those boot-time requests here.
        self.apply_window_requests(event_loop);
        self.apply_extension_effects(event_loop);
        if self.windows.is_empty() && self.hidden_windows.is_empty() {
            self.shutdown_extensions(event_loop);
            event_loop.exit();
        }
    }

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        for app in self.windows.values_mut() {
            app.destroy_surfaces(event_loop);
        }
        self.windows.clear();
        self.hidden_windows.clear();
        self.window_resources.clear();
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Complete bounded extension -> runtime -> extension handshakes in one
        // native wake. A JavaScript promise resumed by an extension can enqueue
        // its next request while this callback is still active; asking the
        // event-loop proxy to wake again there may be coalesced by winit.
        const MAX_WAKE_ROUNDS: usize = 32;
        for round in 0..MAX_WAKE_ROUNDS {
            self.poll_extensions(event_loop);
            let mut progressed = false;
            for app in self.windows.values_mut() {
                progressed |= app.drain_proxy_wake_up();
            }
            for app in self.hidden_windows.values_mut() {
                progressed |= app.drain_proxy_wake_up();
            }
            self.apply_extension_effects(event_loop);
            self.apply_window_requests(event_loop);
            if !progressed {
                break;
            }
            if round + 1 == MAX_WAKE_ROUNDS {
                tracing::warn!(
                    rounds = MAX_WAKE_ROUNDS,
                    "native wake handshake reached its bounded drain limit"
                );
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) {
            let Some(window_key) = self.windows.get(&window_id).map(|app| app.window_key) else {
                return;
            };
            self.request_window_close(window_key, event_loop);
            if self.windows.is_empty() && self.hidden_windows.is_empty() {
                self.shutdown_extensions(event_loop);
                event_loop.exit();
            }
            return;
        }
        let metrics_changed = matches!(
            &event,
            WindowEvent::SurfaceResized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::ThemeChanged(_)
        );
        let previous_metrics = self.windows.get(&window_id).map(|app| app.window_metrics);
        if let WindowEvent::PointerMoved {
            position,
            primary: true,
            ..
        } = &event
            && let Some(app) = self.windows.get(&window_id)
        {
            let window_key = app.window_key;
            let position = app.logical_pointer_position(*position);
            let mut context =
                Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
            for extension in &mut self.extensions {
                extension.pointer_moved(window_key, position, &mut context);
            }
        }
        if matches!(&event, WindowEvent::PointerLeft { primary: true, .. })
            && let Some(app) = self.windows.get(&window_id)
        {
            let window_key = app.window_key;
            let mut context =
                Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
            for extension in &mut self.extensions {
                extension.pointer_left(window_key, &mut context);
            }
        }
        if let WindowEvent::PointerButton {
            state,
            button,
            position,
            ..
        } = &event
        {
            let Some(window_key) = self.windows.get(&window_id).map(|app| app.window_key) else {
                return;
            };
            let position = self
                .windows
                .get(&window_id)
                .map(|app| app.logical_pointer_position(*position))
                .unwrap_or_default();
            let button = pointer_button(button);
            let phase = match state {
                ElementState::Pressed => PointerPhase::Down,
                ElementState::Released => PointerPhase::Up,
            };
            let mut context =
                Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
            if self.extensions.iter_mut().any(|extension| {
                extension.pointer_button(window_key, button, phase, position, &mut context)
            }) {
                return;
            }
        }
        let geometry_changed = self
            .windows
            .get_mut(&window_id)
            .is_some_and(|app| app.handle_window_event(event_loop, window_id, event));
        if metrics_changed
            && let Some(window_key) = self
                .windows
                .get(&window_id)
                .filter(|app| Some(app.window_metrics) != previous_metrics)
                .map(|app| app.window_key)
        {
            let mut context =
                Self::extension_context(&mut self.windows, &mut self.hidden_windows, event_loop);
            for extension in &mut self.extensions {
                extension.window_metrics_changed(window_key, &mut context);
            }
            // Geometry observers may switch the view matching the new viewport.
            // Consume that invalidation before a queued redraw, not on a later
            // proxy wake (which may be coalesced while handling this event).
            if let Some(app) = self.windows.get_mut(&window_id) {
                app.drain_proxy_wake_up();
                if geometry_changed {
                    // Submit the first frame at the new extent in this event,
                    // rather than exposing the resized window with its old
                    // buffer until a later RedrawRequested is dispatched.
                    app.redraw();
                }
            }
        }
        self.apply_extension_effects(event_loop);
        self.apply_window_requests(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        let now = Instant::now();
        let mut earliest = None;
        for app in self.windows.values_mut() {
            match app.next_frame_wake(now) {
                FrameWake::Redraw => {
                    if let Some(shell) = app.state.as_ref() {
                        shell.window().request_redraw();
                    }
                }
                FrameWake::Deadline(deadline) => {
                    earliest = Some(earliest.map_or(deadline, |old: Instant| old.min(deadline)));
                }
                FrameWake::Idle => {}
            }
        }
        event_loop.set_control_flow(earliest.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}
