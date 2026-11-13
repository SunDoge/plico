use super::*;

impl App {
    pub(super) fn redraw(&mut self) {
        let frame_measurement = crate::telemetry::span("frame", "redraw.cpu");
        self.performance_hud.begin_frame(Instant::now());
        #[cfg(feature = "profiling")]
        let frame_span = tracing::trace_span!(
            target: "plico::perf",
            "frame",
            window_id = %self.window_key,
            node_count = tracing::field::Empty,
            build_ms = tracing::field::Empty,
            declaration_ms = tracing::field::Empty,
            sync_ms = tracing::field::Empty,
            backend_ms = tracing::field::Empty,
            scene_ms = tracing::field::Empty,
            present_ms = tracing::field::Empty,
        );
        #[cfg(feature = "profiling")]
        let _frame_guard = frame_span.enter();
        let resized = self
            .state
            .as_mut()
            .is_some_and(|shell| shell.sync_surface_size());
        if resized {
            self.sync_window_metrics();
        }
        let Some(shell) = self.state.as_mut() else {
            return;
        };
        let semantics_enabled = self.force_semantics || shell.accessibility.prepare_frame();
        self.source.set_semantics_enabled(semantics_enabled);
        for action in shell.accessibility.take_actions() {
            if let Err(source) = self.source.prepare_for_event(&mut shell.tcx) {
                crate::telemetry::report_error(
                    "accessibility event preparation rejected",
                    &crate::Error::Frame { source },
                );
                break;
            }
            self.source.handle_semantic_action(action);
        }
        let (node_count, build_frame_ms, scene_ms) =
            match Self::build(shell, self.source.as_mut(), frame_measurement.id()) {
                Ok(frame) => frame,
                Err(error) => {
                    crate::telemetry::report_error("frame construction rejected", &error);
                    if let Some(request) = self.source.take_screenshot_request() {
                        self.source
                            .complete_screenshot(&request.path, Err(error.to_string()));
                    }
                    if shell.initial_show_pending() {
                        self.fail_initial_frame(error);
                    }
                    return;
                }
            };
        let base_color = self.source.base_color();
        self.update_ime_state();
        // Immediate views may produce actions while consuming events during build.
        self.drain_host_actions();
        self.sync_pointer_cursor();
        let Some(shell) = self.state.as_mut() else {
            return;
        };
        shell
            .accessibility
            .set_snapshot(self.source.semantic_snapshot());
        let (logical_width, logical_height) = shell.logical_size();
        let scale = shell.scale_factor();
        let mut hud_stats = self.frame_stats;
        self.source.augment_frame_stats(&mut hud_stats);
        self.performance_hud.paint(
            &mut shell.scene,
            &mut shell.tcx,
            hud_stats,
            [logical_width, logical_height],
            scale,
        );
        let base_color = shell.prepare_window_frame(base_color);
        let captured = if let Some(mut request) = self.source.take_screenshot_request() {
            let (width, height) = shell.size();
            let result = shell
                .renderer
                .capture(&shell.scene, width, height, base_color)
                .and_then(|image| {
                    crate::renderer::write_png_file(&image, &mut request.file, &request.path)
                })
                .map(|_| request.path.clone())
                .map_err(|error| error.to_string());
            drop(request.file);
            self.source.complete_screenshot(&request.path, result);
            true
        } else {
            false
        };
        // AppKit can request redraws for an occluded view, and asynchronous
        // services can wake hidden windows. Keep declarations, host actions,
        // semantics and explicit captures above alive, but do not reacquire a
        // drawable or rebuild trimmed GPU caches just to present out of sight.
        // Hidden startup still needs its first frame before ready-to-show.
        if self.lifecycle.presence() != WindowPresence::Visible
            || self.window_metrics.occluded && !shell.initial_show_pending()
        {
            shell.animation_frame_ready(false);
            if captured {
                // Explicit capture may recreate the offscreen/readback targets
                // after idle trim; it must not leave them pinned out of sight.
                shell.trim_gpu_cache();
            }
            self.present_retry_pending = false;
            self.skipped_present_deadline = None;
            return;
        }
        let t2 = Instant::now();
        let presented = {
            #[cfg(feature = "profiling")]
            let span = tracing::trace_span!(target: "plico::perf", "frame.present");
            #[cfg(feature = "profiling")]
            let _guard = span.enter();
            let mut phase = crate::telemetry::span("frame", "present.cpu");
            phase.attribute("parent", frame_measurement.id());
            shell.present(base_color)
        };
        if let Some(message) = shell.initial_show_error() {
            // A failed hidden startup must not become an invisible animation
            // loop continually submitting work (e.g. after a GPU wait timeout).
            self.fail_initial_frame(crate::Error::Renderer { message });
            return;
        }
        let present_ms = ms(Instant::now() - t2);
        let present_skipped = shell.present_skipped();
        self.skipped_present_deadline = present_skipped.then(|| {
            let hz = shell
                .window()
                .current_monitor()
                .and_then(|m| m.current_video_mode())
                .and_then(|mode| mode.refresh_rate_millihertz())
                .map(|rate| rate.get() as f64 / 1000.)
                .unwrap_or(60.);
            Instant::now() + std::time::Duration::from_secs_f64(1. / hz)
        });
        #[cfg(feature = "profiling")]
        frame_span.record("node_count", node_count as u64);
        #[cfg(feature = "profiling")]
        frame_span.record("build_ms", build_frame_ms);
        #[cfg(feature = "profiling")]
        let source_sample = {
            let mut sample = FrameStats::default();
            self.source.augment_frame_stats(&mut sample);
            sample
        };
        #[cfg(feature = "profiling")]
        frame_span.record("declaration_ms", source_sample.sample_declaration_ms);
        #[cfg(feature = "profiling")]
        frame_span.record("sync_ms", source_sample.sample_sync_ms);
        #[cfg(feature = "profiling")]
        frame_span.record("backend_ms", source_sample.sample_backend_ms);
        #[cfg(feature = "profiling")]
        frame_span.record("scene_ms", scene_ms);
        #[cfg(feature = "profiling")]
        frame_span.record("present_ms", present_ms);
        #[cfg(feature = "profiling")]
        tracing::trace!(
            target: "plico::perf",
            window_id = %self.window_key,
            node_count,
            build_ms = build_frame_ms,
            declaration_ms = source_sample.sample_declaration_ms,
            declaration_passes = source_sample.declaration_passes,
            sync_ms = source_sample.sample_sync_ms,
            backend_ms = source_sample.sample_backend_ms,
            layout_runs = source_sample.layout_runs,
            flatten_visits = source_sample.flatten_visits,
            scene_ms,
            scene_commands = source_sample.scene_commands,
            scene_reused = source_sample.scene_reused,
            present_ms,
            presented = presented && !present_skipped,
            present_skipped,
            frame_accepted = presented,
            "frame.complete"
        );
        self.frame_stats
            .update(build_frame_ms, scene_ms, present_ms, node_count);
        if presented
            && !present_skipped
            && let Some(budget) = shell.frame_budget()
        {
            self.frame_stats.apply_budget(budget);
        }
        self.frame_stats.damage_coverage = if presented {
            shell.damage_coverage()
        } else {
            None
        };
        self.source.augment_frame_stats(&mut self.frame_stats);
        self.source.push_frame_stats(&self.frame_stats);
        if shell.surface_acquire_pending() || shell.presentation_deferred() {
            // The worker will wake us once. Do not turn a normal wait into an
            // immediate failed-present retry and redundant UI/scene work.
            self.present_retry_pending = false;
        } else if update_present_retry(presented, &mut self.present_retry_pending) {
            shell.window().request_redraw();
        }
    }
}
