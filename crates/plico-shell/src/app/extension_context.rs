use super::*;

impl ExtensionContext<'_> {
    /// Serialize the primary window's persisted state, when its source has any.
    pub fn persisted_snapshot(&mut self) -> Option<Vec<u8>> {
        self.windows
            .values_mut()
            .next()
            .and_then(|app| app.source.persisted_snapshot())
    }
    /// Replace the primary window's persisted state before the first frame.
    pub fn restore_persisted(&mut self, bytes: &[u8]) -> bool {
        self.windows.values_mut().next().is_some_and(|app| {
            app.source.restore_persisted(bytes);
            true
        })
    }
    /// Render the latest completed native scene for a visible window.
    pub fn render_screenshot(
        &mut self,
        window_key: WindowResourceKey,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let app = find_window_by_key(self.windows.values_mut(), window_key)
            .ok_or_else(|| "native window is not visible".to_owned())?;
        let base_color = app.source.base_color();
        let shell = app
            .state
            .as_mut()
            .ok_or_else(|| "native window has no render surface".to_owned())?;
        let (width, height) = shell.size();
        shell
            .renderer
            .capture(&shell.scene, width, height, base_color)
            .and_then(|image| crate::renderer::save_png(&image, &path.to_string_lossy()))
            .map_err(|error| error.to_string())
    }

    /// Return the latest logical and physical metrics for a visible window.
    pub fn window_metrics(&self, window_key: WindowResourceKey) -> Option<WindowMetrics> {
        self.windows
            .values()
            .find(|app| app.window_key == window_key)
            .map(|app| app.window_metrics)
    }

    /// Request a new logical inner size for a visible native window.
    ///
    /// The platform remains authoritative: minimum/maximum constraints are
    /// applied by winit and the resulting surface resize travels through the
    /// normal `WindowEvent::SurfaceResized` path.
    pub fn resize_window(
        &mut self,
        window_key: WindowResourceKey,
        width: u32,
        height: u32,
    ) -> bool {
        if width == 0 || height == 0 {
            return false;
        }
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        let Some(shell) = app.state.as_mut() else {
            return false;
        };
        if let Some(size) = shell
            .window()
            .request_surface_size(LogicalSize::new(f64::from(width), f64::from(height)).into())
        {
            // Some platforms apply the size synchronously and need not emit a
            // SurfaceResized event. Never keep rendering at the old extent.
            shell.resize(size.width, size.height);
            app.sync_window_metrics();
        }
        true
    }

    /// Begin an OS-managed move from the current pointer-down gesture.
    /// A macOS titlebar double click can consume it as the system's action.
    pub fn start_window_drag(&mut self, window_key: WindowResourceKey) -> bool {
        find_window_by_key(self.windows.values_mut(), window_key)
            .and_then(|app| app.state.as_mut())
            .is_some_and(|shell| shell.start_window_drag())
    }

    /// Request a physical desktop position, clamped to an available monitor.
    /// Compositors that do not expose absolute positions return false.
    pub fn position_window(&mut self, window_key: WindowResourceKey, x: i32, y: i32) -> bool {
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        let Some(shell) = app.state.as_ref() else {
            return false;
        };
        if shell.window().outer_position().is_err() {
            return false;
        }
        let monitors: Vec<_> = self
            .event_loop
            .available_monitors()
            .filter_map(|monitor| Some((monitor.position()?, monitor.current_video_mode()?.size())))
            .collect();
        let monitor = monitors
            .iter()
            .find(|(origin, size)| {
                i64::from(x) >= i64::from(origin.x)
                    && i64::from(y) >= i64::from(origin.y)
                    && i64::from(x) < i64::from(origin.x) + i64::from(size.width)
                    && i64::from(y) < i64::from(origin.y) + i64::from(size.height)
            })
            .or_else(|| monitors.first());
        let (x, y) = monitor.map_or((x, y), |(origin, size)| {
            let (width, height) = shell.size();
            let right = (i64::from(origin.x) + i64::from(size.width.saturating_sub(width)))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            let bottom = (i64::from(origin.y) + i64::from(size.height.saturating_sub(height)))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            (
                x.clamp(origin.x, right.max(origin.x)),
                y.clamp(origin.y, bottom.max(origin.y)),
            )
        });
        shell
            .window()
            .set_outer_position(winit::dpi::PhysicalPosition::new(x, y).into());
        true
    }

    /// Place a utility window near the upper-right of its current monitor.
    pub fn place_window_top_right(&mut self, window_key: WindowResourceKey, margin: u32) -> bool {
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        let Some(shell) = app.state.as_ref() else {
            return false;
        };
        let Some(monitor) = shell
            .window()
            .current_monitor()
            .or_else(|| self.event_loop.primary_monitor())
        else {
            return false;
        };
        let (Some(origin), Some(mode)) = (monitor.position(), monitor.current_video_mode()) else {
            return false;
        };
        let margin = (f64::from(margin) * monitor.scale_factor()).round() as u32;
        let size = mode.size();
        let (width, _) = shell.size();
        let x = (i64::from(origin.x)
            + i64::from(size.width.saturating_sub(width).saturating_sub(margin)))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let y = origin.y.saturating_add(margin.min(i32::MAX as u32) as i32);
        self.position_window(window_key, x, y)
    }

    /// Return the latest immutable semantic snapshot for a visible window.
    ///
    /// Extensions should treat values as potentially sensitive and avoid
    /// persisting them unless their API explicitly opts into doing so.
    pub fn semantic_snapshot(
        &mut self,
        window_key: WindowResourceKey,
    ) -> Option<Arc<crate::SemanticSnapshot>> {
        find_window_by_key(self.windows.values_mut(), window_key)?
            .source
            .semantic_snapshot()
    }

    /// Return the unique matching semantic node from the latest retained snapshot.
    ///
    /// Callers that dispatch interaction must reject disabled nodes. Snapshot
    /// queries intentionally retain them so test assertions can inspect the
    /// disabled state. Ambiguous matches return `None` rather than implicitly
    /// selecting whichever node happens to appear first.
    pub fn semantic_node_by_role(
        &mut self,
        window_key: WindowResourceKey,
        role: &str,
        label: &str,
    ) -> Option<crate::SemanticNode> {
        let snapshot = self.semantic_snapshot(window_key)?;
        let role = SemanticRole::from_name(role)?;
        snapshot.node_by_role(role, label, None).cloned()
    }

    /// Return one explicit zero-based occurrence in semantic source order.
    pub fn semantic_node_by_role_at(
        &mut self,
        window_key: WindowResourceKey,
        role: &str,
        label: &str,
        index: usize,
    ) -> Option<crate::SemanticNode> {
        let snapshot = self.semantic_snapshot(window_key)?;
        let role = SemanticRole::from_name(role)?;
        snapshot.node_by_role(role, label, Some(index)).cloned()
    }

    /// Return whether the latest semantic snapshot focuses `node_id`.
    pub fn semantic_node_focused(&mut self, window_key: WindowResourceKey, node_id: u64) -> bool {
        find_window_by_key(self.windows.values_mut(), window_key)
            .and_then(|app| app.source.semantic_snapshot())
            .is_some_and(|snapshot| snapshot.focus == Some(node_id))
    }

    /// Deliver synthetic input through the same frame-source path as winit.
    pub fn dispatch_event(&mut self, window_key: WindowResourceKey, event: UiEvent) -> bool {
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        let response = app.dispatch_event(event);
        if response.request_redraw
            && let Some(shell) = app.state.as_ref()
        {
            shell.window().request_redraw();
        }
        true
    }

    /// Route semantic focus to a node in a visible logical window.
    pub fn focus_semantic_node(&mut self, window_key: WindowResourceKey, node_id: u64) -> bool {
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        if let Some(shell) = app.state.as_mut() {
            app.source.prepare_for_event(&mut shell.tcx);
        }
        app.source
            .handle_semantic_action(SemanticAction::Focus { target: node_id })
    }

    /// Dispatch an explicit semantic action to a visible logical window.
    ///
    /// Native automation adapters use this to share the same focus and
    /// scroll-into-view behavior as platform accessibility clients.
    pub fn dispatch_semantic_action(
        &mut self,
        window_key: WindowResourceKey,
        action: SemanticAction,
    ) -> bool {
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        if let Some(shell) = app.state.as_mut() {
            app.source.prepare_for_event(&mut shell.tcx);
        }
        app.source.handle_semantic_action(action)
    }

    /// Find one uniquely matching enabled semantic node and activate it through
    /// the normal pointer hit-test and event-dispatch path.
    pub fn click_by_role(
        &mut self,
        window_key: WindowResourceKey,
        role: &str,
        label: &str,
    ) -> bool {
        self.click_by_role_occurrence(window_key, role, label, None)
    }

    /// Activate one explicit zero-based occurrence in semantic source order.
    pub fn click_by_role_at(
        &mut self,
        window_key: WindowResourceKey,
        role: &str,
        label: &str,
        index: usize,
    ) -> bool {
        self.click_by_role_occurrence(window_key, role, label, Some(index))
    }

    fn click_by_role_occurrence(
        &mut self,
        window_key: WindowResourceKey,
        role: &str,
        label: &str,
        index: Option<usize>,
    ) -> bool {
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        let Some(snapshot) = app.source.semantic_snapshot() else {
            return false;
        };
        let Some(node) = SemanticRole::from_name(role)
            .and_then(|role| snapshot.node_by_role(role, label, index))
        else {
            return false;
        };
        if node.disabled {
            return false;
        }
        let position = Point {
            x: f64::from((node.bounds[0] + node.bounds[2]) * 0.5),
            y: f64::from((node.bounds[1] + node.bounds[3]) * 0.5),
        };
        app.dispatch_event(UiEvent::Pointer(PointerEvent {
            phase: PointerPhase::Down,
            position,
            button: Some(PointerButton::Primary),
            buttons: 1,
            modifiers: Modifiers::default(),
            properties: PointerProperties {
                id: namespaced_pointer_id(0, 1),
                pointer_type: PointerType::Mouse,
                primary: true,
                ..PointerProperties::default()
            },
        }));
        let response = app.dispatch_event(UiEvent::Pointer(PointerEvent {
            phase: PointerPhase::Up,
            position,
            button: Some(PointerButton::Primary),
            buttons: 0,
            modifiers: Modifiers::default(),
            properties: PointerProperties {
                id: namespaced_pointer_id(0, 1),
                pointer_type: PointerType::Mouse,
                primary: true,
                ..PointerProperties::default()
            },
        }));
        if response.request_redraw
            && let Some(shell) = app.state.as_ref()
        {
            shell.window().request_redraw();
        }
        true
    }

    /// Route an asynchronous effect completion to a visible window's source.
    pub fn complete_effect(
        &mut self,
        window_key: WindowResourceKey,
        completion: crate::EffectCompletion,
    ) -> bool {
        let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) else {
            return false;
        };
        app.source.complete_effect(completion);
        true
    }

    /// Return a visible window's raw handle for a platform extension.
    pub fn window_handle(&self, window_key: WindowResourceKey) -> Option<RawWindowHandle> {
        self.windows
            .values()
            .find(|app| app.window_key == window_key)?
            .state
            .as_ref()?
            .window()
            .window_handle()
            .ok()
            .map(|handle| handle.as_raw())
    }

    /// Return a visible window's physical-pixels-per-logical-pixel scale.
    pub fn window_scale_factor(&self, window_key: WindowResourceKey) -> Option<f64> {
        self.windows
            .values()
            .find(|app| app.window_key == window_key)?
            .state
            .as_ref()
            .map(|shell| shell.scale_factor())
    }

    /// Return lifecycle state for a visible or surface-released window.
    pub fn window_lifecycle(&self, window_key: WindowResourceKey) -> Option<WindowLifecycle> {
        self.windows
            .values()
            .find(|app| app.window_key == window_key)
            .or_else(|| self.hidden_windows.get(&window_key))
            .map(|app| app.lifecycle)
    }

    /// Show a hidden window or recreate a previously released surface.
    pub fn show_window(&mut self, window_key: WindowResourceKey) -> bool {
        if let Some(app) = find_window_by_key(self.windows.values_mut(), window_key) {
            let Some(shell) = app.state.as_mut() else {
                return false;
            };
            let capabilities = window_capabilities(
                shell
                    .window()
                    .window_handle()
                    .ok()
                    .map(|handle| handle.as_raw()),
            );
            return match app.lifecycle.transition(WindowIntent::Show, capabilities) {
                Some(WindowEffect::SetVisible(true)) => {
                    app.skipped_present_deadline = None;
                    shell.invalidate_presentation();
                    shell.window().set_visible(true);
                    shell.window().set_minimized(false);
                    shell.window().focus_window();
                    shell.window().request_redraw();
                    true
                }
                None => {
                    shell.window().set_minimized(false);
                    shell.window().focus_window();
                    shell.window().request_redraw();
                    true
                }
                _ => false,
            };
        }

        let Some(mut app) = self.hidden_windows.remove(&window_key) else {
            return false;
        };
        if app
            .lifecycle
            .transition(WindowIntent::Show, WindowCapabilities::default())
            != Some(WindowEffect::RecreateSurface)
        {
            self.hidden_windows.insert(window_key, app);
            return false;
        }
        app.can_create_surfaces(self.event_loop);
        let Some(window_id) = app.state.as_ref().map(|shell| shell.window().id()) else {
            self.hidden_windows.insert(window_key, app);
            return false;
        };
        self.windows.insert(window_id, app);
        true
    }

    /// Hide a logical window using capabilities derived from its native handle.
    pub fn hide_window(&mut self, window_key: WindowResourceKey) -> bool {
        self.hide_window_with_capabilities(window_key, None)
    }

    /// Hide using an optional capability override. The override exists for the
    /// deterministic test host; production derives capabilities from the real
    /// native handle.
    pub fn hide_window_with_capabilities(
        &mut self,
        window_key: WindowResourceKey,
        override_capabilities: Option<WindowCapabilities>,
    ) -> bool {
        let Some(window_id) = self
            .windows
            .iter()
            .find_map(|(window_id, app)| (app.window_key == window_key).then_some(*window_id))
        else {
            return false;
        };
        let Some(handle) = self
            .windows
            .get(&window_id)
            .and_then(|app| app.state.as_ref())
            .and_then(|shell| shell.window().window_handle().ok())
            .map(|handle| handle.as_raw())
        else {
            return false;
        };

        let capabilities =
            override_capabilities.unwrap_or_else(|| window_capabilities(Some(handle)));
        let effect = self
            .windows
            .get_mut(&window_id)
            .and_then(|app| app.lifecycle.transition(WindowIntent::Hide, capabilities));
        match effect {
            Some(WindowEffect::ReleaseSurface) => {
                let Some(mut app) = self.windows.remove(&window_id) else {
                    return false;
                };
                app.destroy_surfaces(self.event_loop);
                self.hidden_windows.insert(window_key, app);
                true
            }
            Some(WindowEffect::SetVisible(false)) => self
                .windows
                .get_mut(&window_id)
                .and_then(|app| app.state.as_mut())
                .is_some_and(|shell| {
                    shell.prepare_hide();
                    shell.window().set_visible(false);
                    true
                }),
            None => true,
            _ => false,
        }
    }

    /// Request orderly termination of the shared native event loop.
    pub fn exit(&self) {
        self.event_loop.exit();
    }
}
