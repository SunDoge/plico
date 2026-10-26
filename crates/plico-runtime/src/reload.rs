//! UI-thread replay of an application module; the rendering host never restarts.
use crate::View;
use notify::Watcher;
use plico_core::{Key, PathKey, Theme, UiContext, ViewNode};
use plico_dev::host::{Descriptor, Module};
use plico_dev::{
    Frame, Request, StateSnapshot,
    services::{HostService, NoServices},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Host-side native input state for one logical path.
#[derive(Default)]
struct HostInput {
    buffer: plico_core::TextBuffer,
    module_rev: u64,
    sent_rev: u64,
    sent_submitted: u64,
    placeholder: String,
}
impl HostInput {
    fn new(buffer: plico_core::TextBuffer, module_rev: u64) -> Self {
        Self {
            sent_rev: buffer.value_revision(),
            sent_submitted: buffer.submitted_count(),
            buffer,
            module_rev,
            placeholder: String::new(),
        }
    }
}
fn sync_inputs(texts: &mut HashMap<Vec<PathKey>, HostInput>, frame: &Frame) {
    for (path, content, revision) in &frame.texts {
        let entry=texts.entry(path.clone()).or_insert_with(|| {
                if content.is_none() {
                    eprintln!("[plico] password input is disabled in hot-reload preview; use the native application for secret entry");
                }
                HostInput::new(if content.is_some() {plico_core::TextBuffer::new()} else {plico_core::TextBuffer::secret()},0)
            });
        if entry.buffer.is_secret() != content.is_none() {
            entry.buffer.clear();
            *entry = HostInput::new(
                if content.is_some() {
                    plico_core::TextBuffer::new()
                } else {
                    plico_core::TextBuffer::secret()
                },
                0,
            );
        }
        if *revision != entry.module_rev {
            if let Some(content) = content
                && entry.buffer.text().as_deref() != Some(content.as_str())
                && entry.buffer.value_revision() == entry.sent_rev
            {
                entry.buffer.set(zeroize::Zeroizing::new(content.clone()));
                entry.sent_rev = entry.buffer.value_revision();
            }
            entry.module_rev = *revision;
        }
        entry.placeholder = frame
            .text_hints
            .iter()
            .find(|(p, _)| p == path)
            .map_or(String::new(), |(_, hint)| hint.clone());
    }
}

pub struct ReloadView<S: HostService = NoServices> {
    services: S,
    services_dirty: bool,
    descriptor: PathBuf,
    active_path: PathBuf,
    module: Module,
    // Keep old code mapped: TLS destructors must never jump into an unloaded image.
    retired: Vec<Module>,
    frame: Frame,
    // Last known-good application state; lean frames skip exchanging it.
    state: StateSnapshot,
    // Native input bindings by logical path: host-side buffer plus the
    // module revision last applied and the host revision/submit count last
    // sent. Edits here dispatch to the module; module edits sync back.
    texts: HashMap<Vec<PathKey>, HostInput>,
    session_valid: bool,
    pending: Vec<Vec<PathKey>>,
    pending_values: Vec<(Vec<PathKey>, f64)>,
    keys: HashMap<Vec<PathKey>, u64>,
    next_key: u64,
    scroll_viewports: Vec<(Vec<PathKey>, [f32; 2])>,
    time: plico_core::FrameTime,
    rendered_time: plico_core::FrameTime,
    viewport: [f32; 2],
    rendered_scrolls: Vec<(Vec<PathKey>, [f32; 2])>,
    changed: Arc<AtomicBool>,
    watcher: Option<notify::RecommendedWatcher>,
    pub reloads: u64,
    pub last_error: Option<String>,
}
impl ReloadView<NoServices> {
    pub fn new(descriptor: PathBuf) -> plico_dev::host::Result<Self> {
        Self::with_services(descriptor, NoServices)
    }
}
impl<S: HostService> ReloadView<S> {
    pub fn with_services(descriptor: PathBuf, mut services: S) -> plico_dev::host::Result<Self> {
        let d = Descriptor::read(&descriptor)?;
        if d.version != plico_dev::PROTOCOL_VERSION {
            return Err(plico_dev::host::Error::Tree {
                message: "descriptor protocol mismatch".into(),
            });
        }
        services.poll();
        let module = Module::load(&d.library)?;
        let frame = module.call(&Request {
            services: serde_json::to_value(services.snapshot())
                .map_err(|source| plico_dev::host::Error::Json { source })?,
            ..Default::default()
        })?;
        reject_preview_commands(&frame)?;
        let state = frame.state.clone();
        let mut view = Self {
            services,
            services_dirty: false,
            descriptor,
            active_path: d.library,
            module,
            retired: Vec::new(),
            frame,
            state,
            texts: HashMap::new(),
            session_valid: true,
            pending: Vec::new(),
            pending_values: Vec::new(),
            keys: HashMap::new(),
            next_key: 0,
            scroll_viewports: Vec::new(),
            time: Default::default(),
            rendered_time: Default::default(),
            viewport: [720., 820.],
            rendered_scrolls: Vec::new(),
            changed: Arc::new(AtomicBool::new(false)),
            watcher: None,
            reloads: 0,
            last_error: None,
        };
        view.sync_inputs();
        Ok(view)
    }
    pub fn services(&self) -> &S {
        &self.services
    }
    /// Mutating host state requires request_services_update, or an actor wake/poll.
    pub fn services_mut(&mut self) -> &mut S {
        &mut self.services
    }
    pub fn request_services_update(&mut self) {
        self.services_dirty = true;
    }
    pub fn state(&self) -> &StateSnapshot {
        &self.state
    }
    pub fn snapshot(&self) -> &Frame {
        &self.frame
    }
    pub fn activate(&mut self) -> plico_dev::host::Result<bool> {
        let d = Descriptor::read(&self.descriptor)?;
        if d.library == self.active_path {
            return Ok(false);
        }
        if d.version != plico_dev::PROTOCOL_VERSION {
            return Err(plico_dev::host::Error::Tree {
                message: "descriptor protocol mismatch".into(),
            });
        }
        let module = Module::load(&d.library)?;
        let state = if self.session_valid {
            self.module
                .call(&Request {
                    checkpoint: true,
                    resume: true,
                    ..Default::default()
                })?
                .state
        } else {
            self.state.clone()
        };
        let frame = module.call(&Request {
            state,
            texts: self.input_snapshot(true),
            text_baselines: self
                .texts
                .iter()
                .map(|(path, input)| (path.clone(), input.buffer.submitted_count()))
                .collect(),
            services: serde_json::to_value(self.services.snapshot())
                .map_err(|source| plico_dev::host::Error::Json { source })?,
            viewport: Some(self.viewport),
            scroll_viewports: self.scroll_viewports.clone(),
            time: self.time,
            ..Default::default()
        })?;
        reject_preview_commands(&frame)?;
        if let Some(reason) = &frame.reset {
            eprintln!("[plico] application state reset: {reason}");
        }
        self.retired
            .push(std::mem::replace(&mut self.module, module));
        self.state = frame.state.clone();
        self.frame = frame;
        self.session_valid = true;
        for input in self.texts.values_mut() {
            input.module_rev = 0;
        }
        self.sync_inputs();
        self.rendered_time = self.time;
        self.rendered_scrolls = self.scroll_viewports.clone();
        self.active_path = d.library;
        self.reloads += 1;
        self.last_error = None;
        eprintln!("[plico] activated application revision {}", self.reloads);
        Ok(true)
    }
    pub fn dispatch(
        &mut self,
        clicks: Vec<Vec<PathKey>>,
        values: Vec<(Vec<PathKey>, f64)>,
    ) -> plico_dev::host::Result<()> {
        let edited = self.texts.values().any(|input| {
            !input.buffer.is_secret()
                && (input.buffer.value_revision() != input.sent_rev
                    || input.buffer.submitted_count() != input.sent_submitted)
        });
        let lean = self.session_valid
            && clicks.is_empty()
            && values.is_empty()
            && !self.services_dirty
            && !edited;
        let texts = self.input_snapshot(!self.session_valid);
        let baselines = if self.session_valid {
            Vec::new()
        } else {
            self.texts
                .iter()
                .map(|(path, input)| (path.clone(), input.sent_submitted))
                .collect()
        };
        let sent: Vec<_> = self
            .texts
            .iter()
            .map(|(path, input)| (path.clone(), input.sent_rev, input.sent_submitted))
            .collect();
        for input in self.texts.values_mut() {
            input.sent_rev = input.buffer.value_revision();
            input.sent_submitted = input.buffer.submitted_count();
        }
        let result = (|| {
            let frame = self.module.call(&Request {
                state: if lean {
                    StateSnapshot::default()
                } else {
                    self.state.clone()
                },
                services: serde_json::to_value(self.services.snapshot())
                    .map_err(|source| plico_dev::host::Error::Json { source })?,
                viewport: Some(self.viewport),
                scroll_viewports: self.scroll_viewports.clone(),
                time: self.time,
                clicks,
                values,
                lean,
                resume: self.session_valid,
                texts,
                text_baselines: baselines,
                ..Default::default()
            })?;
            self.accept_frame(frame)
        })();
        self.session_valid = result.is_ok();
        if result.is_err() {
            for (path, revision, submitted) in sent {
                if let Some(input) = self.texts.get_mut(&path) {
                    input.sent_rev = revision;
                    input.sent_submitted = submitted;
                }
            }
        }
        result
    }
    fn input_snapshot(&self, all: bool) -> Vec<(Vec<PathKey>, String, u64)> {
        self.texts
            .iter()
            .filter(|(_, input)| {
                !input.buffer.is_secret()
                    && (all
                        || input.buffer.value_revision() != input.sent_rev
                        || input.buffer.submitted_count() != input.sent_submitted)
            })
            .map(|(path, input)| {
                (
                    path.clone(),
                    input.buffer.text().unwrap_or_default(),
                    input.buffer.submitted_count(),
                )
            })
            .collect()
    }
    fn sync_inputs(&mut self) {
        sync_inputs(&mut self.texts, &self.frame);
    }
    fn accept_frame(&mut self, mut frame: Frame) -> plico_dev::host::Result<()> {
        if frame.unmatched_events > 0 {
            // Path identity no longer matches any declaration: input was dropped,
            // most likely because an edit inserted or removed a sibling control.
            eprintln!(
                "[plico] dropped {} input event(s) without a matching control; declaration identity may have drifted after an edit",
                frame.unmatched_events
            );
        }
        // Decode the whole batch before committing UI or executing any command.
        let commands = decode_commands::<S::Command>(std::mem::take(&mut frame.commands))?;
        if !frame.state.is_empty() {
            self.state = frame.state.clone();
        }
        self.frame = frame;
        self.sync_inputs();
        self.last_error = None;
        self.rendered_time = self.time;
        self.rendered_scrolls = self.scroll_viewports.clone();
        self.services_dirty = !commands.is_empty();
        for command in commands {
            self.services.send(command);
        }
        Ok(())
    }
    fn replay(&mut self, ui: &mut UiContext<'_>) {
        struct Replay<'a> {
            keys: &'a mut HashMap<Vec<PathKey>, u64>,
            next: &'a mut u64,
            clicks: &'a mut Vec<Vec<PathKey>>,
            values: &'a mut Vec<(Vec<PathKey>, f64)>,
            scrolls: &'a mut Vec<(Vec<PathKey>, [f32; 2])>,
            texts: &'a HashMap<Vec<PathKey>, HostInput>,
        }
        impl Replay<'_> {
            fn nodes(
                &mut self,
                ui: &mut UiContext<'_>,
                nodes: &[ViewNode],
                path: &mut Vec<PathKey>,
            ) {
                for n in nodes {
                    path.push(n.key.clone());
                    let key = *self.keys.entry(path.clone()).or_insert_with(|| {
                        *self.next += 1;
                        *self.next
                    });
                    if n.kind == plico_core::Kind::TextInput {
                        // Bind the host-side native editor so the adapter's
                        // input handling (focus, edits, IME) reaches it; the
                        // transport syncs content with the module's buffer.
                        if let Some(input) = self.texts.get(path) {
                            let buffer = input.buffer.clone();
                            let mut style = n.style;
                            style.input_revision = buffer.revision();
                            style.disabled |= buffer.is_secret();
                            ui.element_with_grid(
                                Key::Number(key),
                                n.kind,
                                style,
                                &n.text,
                                n.grid.as_ref(),
                                |ui| {
                                    ui.bind_input(&buffer, &input.placeholder);
                                    self.nodes(ui, &n.children, path);
                                },
                            );
                        } else {
                            // No host binding yet: declare the node; the
                            // module's next frame reports its input state and
                            // the binding installs on the following replay.
                            let _ = ui.element_with_grid(
                                Key::Number(key),
                                n.kind,
                                n.style,
                                &n.text,
                                n.grid.as_ref(),
                                |ui| {
                                    self.nodes(ui, &n.children, path);
                                },
                            );
                        }
                        path.pop();
                        continue;
                    }
                    if n.kind == plico_core::Kind::Shader {
                        if let Some(params) = &n.shader {
                            ui.shader_element(Key::Number(key), n.style, &n.text, params);
                        }
                    } else if n.kind == plico_core::Kind::Text {
                        ui.rich_text_element(Key::Number(key), n.style, &n.text, &n.spans);
                    } else if n.kind == plico_core::Kind::Slider {
                        if let Some(value) = n.numeric.and_then(|range| {
                            ui.slider_element(Key::Number(key), n.style, &n.text, range)
                        }) {
                            self.values.push((path.clone(), value));
                        }
                    } else {
                        let (_, clicked) = ui.element_with_grid(
                            Key::Number(key),
                            n.kind,
                            n.style,
                            &n.text,
                            n.grid.as_ref(),
                            |ui| {
                                if n.style.scroll_y
                                    && let Some(viewport) = ui.scroll_viewport()
                                {
                                    if let Some((_, old)) =
                                        self.scrolls.iter_mut().find(|(p, _)| p == path)
                                    {
                                        if *old != viewport {
                                            *old = viewport;
                                            ui.request_rebuild();
                                        }
                                    } else {
                                        self.scrolls.push((path.clone(), viewport));
                                        ui.request_rebuild();
                                    }
                                }
                                self.nodes(ui, &n.children, path);
                            },
                        );
                        if clicked {
                            self.clicks.push(path.clone());
                        }
                    }
                    path.pop();
                }
            }
        }
        Replay {
            keys: &mut self.keys,
            next: &mut self.next_key,
            clicks: &mut self.pending,
            values: &mut self.pending_values,
            scrolls: &mut self.scroll_viewports,
            texts: &self.texts,
        }
        .nodes(ui, &self.frame.nodes, &mut Vec::new());
        // Virtualized children are unmounted. Do not retain a path-to-ID entry
        // for every row ever visited during a long session.
        fn active_paths(
            nodes: &[ViewNode],
            path: &mut Vec<PathKey>,
            active: &mut std::collections::HashSet<Vec<PathKey>>,
        ) {
            for node in nodes {
                path.push(node.key.clone());
                active.insert(path.clone());
                active_paths(&node.children, path, active);
                path.pop();
            }
        }
        let mut active = std::collections::HashSet::new();
        active_paths(&self.frame.nodes, &mut Vec::new(), &mut active);
        self.keys.retain(|path, _| active.contains(path));
        self.scroll_viewports
            .retain(|(path, _)| active.contains(path));
        self.texts.retain(|path, _| active.contains(path));
    }
}
impl<S: HostService> View for ReloadView<S> {
    fn on_unhandled_key(&mut self, key: &plico_renderer::KeyEvent) -> bool {
        let services = match serde_json::to_value(self.services.snapshot()) {
            Ok(value) => value,
            Err(error) => {
                self.last_error = Some(error.to_string());
                return false;
            }
        };
        let request = Request {
            services,
            state: self.state.clone(),
            resume: self.session_valid,
            key_events: vec![key.clone()],
            time: self.time,
            viewport: Some(self.viewport),
            scroll_viewports: self.scroll_viewports.clone(),
            ..Default::default()
        };
        match self.module.call(&request) {
            Ok(frame) => {
                let handled = frame.key_handled;
                match self.accept_frame(frame) {
                    Ok(()) => {
                        self.session_valid = true;
                        handled
                    }
                    Err(error) => {
                        self.session_valid = false;
                        self.last_error = Some(error.to_string());
                        false
                    }
                }
            }
            Err(error) => {
                self.session_valid = false;
                self.last_error = Some(error.to_string());
                false
            }
        }
    }
    fn set_frame_time(&mut self, time: plico_core::FrameTime) {
        self.time = time;
    }
    fn show(&mut self, ui: &mut UiContext<'_>) {
        // The module already supplied a settled frame on load/activation.
        // Hover, presentation and animation frames need only replay that frame.
        let resized = self.viewport != ui.viewport_size();
        self.viewport = ui.viewport_size();
        let pending = std::mem::take(&mut self.pending);
        let values = std::mem::take(&mut self.pending_values);
        let had_clicks = !pending.is_empty() || !values.is_empty();
        fn presence_due(nodes: &[ViewNode], now: u128) -> bool {
            nodes.iter().any(|node| {
                node.style
                    .presence_until_ms
                    .is_some_and(|deadline| now >= deadline as u128)
                    || presence_due(&node.children, now)
            })
        }
        let ticking = (self.frame.requests.contains(plico_core::Changes::ANIMATE)
            || presence_due(&self.frame.nodes, self.time.now.as_millis()))
            && self.rendered_time != self.time;
        // Typing edits the host-side binding outside the click/value channels;
        // a revision bump must reach the module like any other input event.
        let text_edited = self.texts.values().any(|input| {
            !input.buffer.is_secret()
                && (input.buffer.value_revision() != input.sent_rev
                    || input.buffer.submitted_count() != input.sent_submitted)
        });
        if (self.services_dirty
            || resized
            || had_clicks
            || text_edited
            || ticking
            || self.rendered_scrolls != self.scroll_viewports)
            && let Err(error) = self.dispatch(pending, values)
        {
            let message = error.to_string();
            if self.last_error.as_ref() != Some(&message) {
                eprintln!("[plico] keeping last good frame: {error}");
            }
            self.last_error = Some(message);
            self.services_dirty = false;
            // A failed ticking application must not spin at vsync forever.
            // A new event or successful reload can request frames again.
            self.frame.requests.remove(plico_core::Changes::ANIMATE);
        }
        self.replay(ui);
        if self.frame.requests.contains(plico_core::Changes::ANIMATE) {
            ui.request_animation_frame();
        }
        if had_clicks || self.services_dirty {
            ui.request_rebuild();
        }
    }
    fn theme(&self) -> Theme {
        self.frame.theme
    }
    fn set_wake_callback(&mut self, wake: plico_renderer::WakeCallback) {
        self.services.set_wake_callback(wake.clone());
        // Cover completions queued between initial projection and callback installation.
        wake();
        let changed = self.changed.clone();
        let descriptor = self.descriptor.clone();
        let result = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|event| descriptor_changed(&event, &descriptor)) {
                changed.store(true, Ordering::Release);
                wake();
            }
        });
        match result {
            Ok(mut watcher) => {
                if let Err(e) = watcher.watch(
                    self.descriptor
                        .parent()
                        .unwrap_or(std::path::Path::new(".")),
                    notify::RecursiveMode::NonRecursive,
                ) {
                    self.last_error = Some(e.to_string());
                } else {
                    self.watcher = Some(watcher);
                }
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }
    fn poll_reload(&mut self) -> bool {
        let service_changed = self.services.poll();
        self.services_dirty |= service_changed;
        if !self.changed.swap(false, Ordering::AcqRel) {
            return service_changed;
        }
        match self.activate() {
            Ok(changed) => changed || service_changed,
            Err(error) => {
                eprintln!(
                    "[plico] rejected reload: {error}; loaded native revisions remain mapped until process exit"
                );
                self.last_error = Some(error.to_string());
                service_changed
            }
        }
    }
}

fn decode_commands<C: serde::de::DeserializeOwned>(
    commands: Vec<serde_json::Value>,
) -> plico_dev::host::Result<Vec<C>> {
    commands
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            serde_json::from_value(value).map_err(|_| plico_dev::host::Error::Tree {
                message: format!(
                    "cannot decode service command {index} as {} (payload withheld)",
                    std::any::type_name::<C>()
                ),
            })
        })
        .collect()
}
fn reject_preview_commands(frame: &Frame) -> plico_dev::host::Result<()> {
    if frame.commands.is_empty() {
        Ok(())
    } else {
        Err(plico_dev::host::Error::Tree {
        message: "module emitted service commands while loading; start services in the host and emit commands only for events".into(),
    })
    }
}
fn descriptor_changed(event: &notify::Event, descriptor: &std::path::Path) -> bool {
    matches!(
        event.kind,
        notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_)
    ) && event.paths.iter().any(|path| {
        path.file_name()
            .is_some_and(|name| Some(name) == descriptor.file_name())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_batch_is_fully_validated_before_delivery_and_preview_is_side_effect_free() {
        #[derive(serde::Deserialize)]
        enum Command {
            Start,
        }
        assert!(
            decode_commands::<Command>(vec![
                serde_json::json!("Start"),
                serde_json::json!("Unknown")
            ])
            .is_err()
        );
        let frame = Frame {
            version: plico_dev::PROTOCOL_VERSION,
            state: StateSnapshot::default(),
            theme: Theme::default(),
            nodes: vec![],
            reset: None,
            commands: vec![serde_json::json!("Start")],
            key_handled: false,
            requests: Default::default(),
            unmatched_events: 0,
            texts: Vec::new(),
            text_hints: Vec::new(),
        };
        assert!(reject_preview_commands(&frame).is_err());
    }
    #[test]
    fn descriptor_reads_and_other_artifacts_do_not_wake_host() {
        use notify::{
            Event, EventKind,
            event::{AccessKind, ModifyKind},
        };
        let event =
            Event::new(EventKind::Access(AccessKind::Any)).add_path("/tmp/active.json".into());
        assert!(!descriptor_changed(
            &event,
            std::path::Path::new("/tmp/active.json")
        ));
        let event =
            Event::new(EventKind::Modify(ModifyKind::Any)).add_path("/tmp/revision-2.so".into());
        assert!(!descriptor_changed(
            &event,
            std::path::Path::new("/tmp/active.json")
        ));
        let event =
            Event::new(EventKind::Modify(ModifyKind::Any)).add_path("/tmp/active.json".into());
        assert!(descriptor_changed(
            &event,
            std::path::Path::new("/tmp/active.json")
        ));
    }
    #[test]
    fn initial_native_inputs_and_module_edits_are_installed_without_echo() {
        let mut frame = Frame {
            version: plico_dev::PROTOCOL_VERSION,
            state: StateSnapshot::default(),
            theme: Theme::default(),
            nodes: Vec::new(),
            reset: None,
            commands: Vec::new(),
            key_handled: false,
            requests: Default::default(),
            unmatched_events: 0,
            texts: vec![(
                vec![PathKey::Named("field".into())],
                Some("initial".into()),
                1,
            )],
            text_hints: vec![(vec![PathKey::Named("field".into())], "Hint".into())],
        };
        let mut inputs = HashMap::new();
        sync_inputs(&mut inputs, &frame);
        let input = &inputs[&frame.texts[0].0];
        assert_eq!(input.buffer.text().as_deref(), Some("initial"));
        assert_eq!(input.placeholder, "Hint");
        assert_eq!(input.sent_rev, input.buffer.value_revision());
        frame.texts[0].1 = Some(String::new());
        frame.texts[0].2 = 2;
        sync_inputs(&mut inputs, &frame);
        let input = &inputs[&frame.texts[0].0];
        assert!(input.buffer.is_empty());
        assert_eq!(
            input.sent_rev,
            input.buffer.value_revision(),
            "module edits must not echo back as host edits"
        );
        frame.texts[0].1 = None;
        frame.texts[0].2 = 3;
        sync_inputs(&mut inputs, &frame);
        assert!(inputs[&frame.texts[0].0].buffer.is_secret());
        assert!(inputs[&frame.texts[0].0].buffer.text().is_none());
    }
    #[test]
    fn failed_command_diagnostic_has_index_and_type_without_payload() {
        let error = decode_commands::<u32>(vec![serde_json::json!("private-credential")])
            .unwrap_err()
            .to_string();
        assert!(error.contains("command 0"));
        assert!(error.contains("u32"));
        assert!(!error.contains("private-credential"));
    }
}
