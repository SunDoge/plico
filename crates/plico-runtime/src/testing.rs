//! Deterministic, renderer-independent application testing.
//!
//! Commands can be replayed from JSON or sent directly from Rust. No window,
//! GPU, OS focus, pixel coordinates or sleeps are needed.
use crate::{HostView, Source};
use plico::{ViewNode, VirtualClock};
use plico_renderer::{
    EventResponse, FrameSource, KeyEvent, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, PointerProperties, SemanticAction, SemanticRole, SemanticSnapshot, TextContext,
    UiEvent, layout::PlacedNode,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::time::Duration;

/// Value-only commands for a test transport. They never contain arena handles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Snapshot,
    /// Move the primary mouse pointer in window-logical pixels.
    PointerMove {
        x: f64,
        y: f64,
    },
    /// Press the primary pointer at window-logical coordinates; release separately.
    PointerDown {
        x: f64,
        y: f64,
    },
    /// Release the primary pointer, allowing intermediate pressed-state captures.
    PointerUp {
        x: f64,
        y: f64,
    },
    /// Remove the pointer from the window (clears hover and pointer queries).
    PointerLeave,
    Click {
        role: String,
        label: String,
        #[serde(default)]
        occurrence: Option<usize>,
    },
    /// Primary mouse activation at the center of a named control. Unlike
    /// [`Self::Click`], this validates that hit testing reaches that control.
    PointerClick {
        role: String,
        label: String,
        #[serde(default)]
        occurrence: Option<usize>,
    },
    /// Secondary mouse activation at the center of a named control. Uses the
    /// actual hit-testing route, never translates into a primary semantic click.
    SecondaryClick {
        role: String,
        label: String,
        #[serde(default)]
        occurrence: Option<usize>,
    },
    Focus {
        role: String,
        label: String,
        #[serde(default)]
        occurrence: Option<usize>,
    },
    SetValue {
        label: String,
        value: f64,
    },
    Text {
        text: String,
    },
    Key {
        key: String,
    },
    /// Full keyboard input for combinations, physical codes and repeat tests.
    KeyEvent {
        event: KeyEvent,
    },
}
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("unknown semantic role: {role}"))]
    UnknownRole { role: String },
    #[snafu(display(
        "target {role} {label:?}: found {matches} matches (specify occurrence for repeated labels)"
    ))]
    Target {
        role: String,
        label: String,
        matches: usize,
    },
    #[snafu(display("control rejected the requested action: {label}"))]
    Rejected { label: String },
    #[snafu(display("UI declaration failed: {message}"))]
    Declaration { message: String },
    #[snafu(display("invalid test viewport; dimensions must be nonzero"))]
    Viewport,
    #[snafu(display("pointer coordinates must be finite"))]
    PointerPosition,
    #[snafu(display(
        "target {role} {label:?} is not hit-testable at ({x:.1}, {y:.1}); hit {hit:?}"
    ))]
    PointerTarget {
        role: String,
        label: String,
        x: f64,
        y: f64,
        hit: Option<u64>,
    },
    #[snafu(display("UI did not settle after {frames} deterministic frames"))]
    DidNotSettle { frames: usize },
    #[snafu(display("virtual clock overflow"))]
    Clock,
}

fn find_target<V: HostView>(
    source: &Source<V>,
    role: &str,
    label: &str,
    occurrence: Option<usize>,
) -> Result<u64, Error> {
    let role_value =
        SemanticRole::from_name(role).ok_or_else(|| Error::UnknownRole { role: role.into() })?;
    let tree = source.adapter.semantics(&source.engine, 0);
    let matches: Vec<_> = tree
        .exposed_nodes()
        .into_iter()
        .filter(|n| n.role == role_value && n.label.as_deref() == Some(label))
        .collect();
    match occurrence {
        Some(index) => matches.get(index).copied(),
        None if matches.len() == 1 => matches.first().copied(),
        _ => None,
    }
    .map(|n| n.id)
    .ok_or_else(|| Error::Target {
        role: role.into(),
        label: label.into(),
        matches: matches.len(),
    })
}

fn target_center<V: HostView>(
    source: &Source<V>,
    role: &str,
    label: &str,
    occurrence: Option<usize>,
) -> Result<(u64, Point), Error> {
    let target = find_target(source, role, label, occurrence)?;
    let tree = source.semantics();
    let bounds = tree
        .nodes
        .iter()
        .find(|node| node.id == target)
        .expect("resolved semantic target must remain in the snapshot")
        .bounds;
    let position = Point {
        x: (f64::from(bounds[0]) + f64::from(bounds[2])) / 2.,
        y: (f64::from(bounds[1]) + f64::from(bounds[3])) / 2.,
    };
    let valid_bounds = bounds.iter().all(|value| value.is_finite())
        && bounds[2] > bounds[0]
        && bounds[3] > bounds[1];
    let hit = valid_bounds
        .then(|| source.hit_test(position).map(plico::Entity::raw))
        .flatten();
    if hit != Some(target) {
        return Err(Error::PointerTarget {
            role: role.into(),
            label: label.into(),
            x: position.x,
            y: position.y,
            hit,
        });
    }
    Ok((target, position))
}

fn pointer_click<V: HostView>(
    source: &mut Source<V>,
    role: &str,
    label: &str,
    occurrence: Option<usize>,
    button: PointerButton,
) -> Result<(), Error> {
    let (_, position) = target_center(source, role, label, occurrence)?;
    source.handle_event(UiEvent::Pointer(PointerEvent {
        phase: PointerPhase::Move,
        position,
        button: None,
        buttons: 0,
        modifiers: Modifiers::empty(),
        properties: PointerProperties::default(),
    }));
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        source.handle_event(UiEvent::Pointer(PointerEvent {
            phase,
            position,
            button: Some(button),
            buttons: if phase == PointerPhase::Down {
                match button {
                    PointerButton::Primary => 1,
                    PointerButton::Secondary => 2,
                    _ => 0,
                }
            } else {
                0
            },
            modifiers: Modifiers::empty(),
            properties: PointerProperties::default(),
        }));
    }
    Ok(())
}

/// Apply a shared command to an existing source, including the development host.
/// The caller builds the resulting frame and owns transport/serialization.
pub fn dispatch<V: HostView>(
    source: &mut Source<V>,
    text: &mut TextContext,
    command: &Command,
) -> Result<(), Error> {
    source
        .try_prepare_for_event(text)
        .map_err(|error| Error::Declaration {
            message: error.to_string(),
        })?;
    match command {
        Command::Snapshot => {}
        Command::PointerMove { x, y } => {
            if !x.is_finite() || !y.is_finite() {
                return Err(Error::PointerPosition);
            }
            source.handle_event(UiEvent::Pointer(PointerEvent {
                phase: PointerPhase::Move,
                position: Point { x: *x, y: *y },
                button: None,
                buttons: u32::from(
                    source.adapter.pressed.is_some()
                        || source.adapter.slider_drag.is_some()
                        || source.adapter.scrollbar_drag.is_some()
                        || source.adapter.backdrop_pressed.is_some(),
                ),
                modifiers: Modifiers::empty(),
                properties: PointerProperties::default(),
            }));
        }
        Command::PointerDown { x, y } | Command::PointerUp { x, y } => {
            if !x.is_finite() || !y.is_finite() {
                return Err(Error::PointerPosition);
            }
            let down = matches!(command, Command::PointerDown { .. });
            source.handle_event(UiEvent::Pointer(PointerEvent {
                phase: if down {
                    PointerPhase::Down
                } else {
                    PointerPhase::Up
                },
                position: Point { x: *x, y: *y },
                button: Some(PointerButton::Primary),
                buttons: u32::from(down),
                modifiers: Modifiers::empty(),
                properties: PointerProperties::default(),
            }));
        }
        Command::PointerLeave => {
            source.handle_event(UiEvent::Pointer(PointerEvent {
                phase: PointerPhase::Leave,
                position: Point { x: 0., y: 0. },
                button: None,
                buttons: 0,
                modifiers: Modifiers::empty(),
                properties: PointerProperties::default(),
            }));
        }
        Command::SecondaryClick {
            role,
            label,
            occurrence,
        } => {
            pointer_click(source, role, label, *occurrence, PointerButton::Secondary)?;
        }
        Command::PointerClick {
            role,
            label,
            occurrence,
        } => {
            pointer_click(source, role, label, *occurrence, PointerButton::Primary)?;
        }
        Command::Click {
            role,
            label,
            occurrence,
        }
        | Command::Focus {
            role,
            label,
            occurrence,
        } => {
            let target = find_target(source, role, label, *occurrence)?;
            let action = if matches!(command, Command::Click { .. }) {
                SemanticAction::Click { target }
            } else {
                SemanticAction::Focus { target }
            };
            if !source.handle_semantic_action(action) {
                return Err(Error::Rejected {
                    label: label.clone(),
                });
            }
        }
        Command::SetValue { label, value } => {
            let target = find_target(source, "slider", label, None)?;
            if !source.handle_semantic_action(SemanticAction::SetNumericValue {
                target,
                value: *value,
            }) {
                return Err(Error::Rejected {
                    label: label.clone(),
                });
            }
        }
        Command::Text { text } => {
            source.handle_event(UiEvent::TextInput(text.clone()));
        }
        Command::KeyEvent { event } => {
            source.handle_event(UiEvent::Key(event.clone()));
        }
        Command::Key { key } => {
            source.handle_event(UiEvent::Key(KeyEvent::logical(
                KeyPhase::Down,
                key.clone(),
                Modifiers::empty(),
            )));
        }
    }
    Ok(())
}

/// A headless application with an explicitly advanced clock. Layout and input
/// use the real runtime; this does not simulate an OS window or GPU swapchain.
pub struct Driver<V: HostView> {
    source: Source<V>,
    text: TextContext,
    clock: VirtualClock,
    size: (u32, u32),
}
impl<V: HostView> Driver<V> {
    pub fn new(view: V, width: u32, height: u32) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::Viewport);
        }
        let clock = VirtualClock::default();
        let mut driver = Self {
            source: Source::with_clock(view, clock.clone()),
            text: TextContext::new(),
            clock,
            size: (width, height),
        };
        driver.frame()?;
        Ok(driver)
    }
    fn frame(&mut self) -> Result<(), Error> {
        self.source
            .try_build_frame(&mut self.text, self.size.0, self.size.1)
            .map_err(|message| Error::Declaration {
                message: message.to_string(),
            })?;
        Ok(())
    }
    /// Current application/host view state after the latest settled frame.
    pub fn view(&self) -> &V {
        self.source.view()
    }

    /// Mutate application state, invalidate declarations and settle one frame.
    pub fn update_view<R>(&mut self, update: impl FnOnce(&mut V) -> R) -> Result<R, Error> {
        let value = self.source.update(update);
        self.frame()?;
        Ok(value)
    }

    /// Retained layout and paint projection for structural assertions.
    pub fn placed_nodes(&self) -> &[PlacedNode] {
        self.source.placed_nodes()
    }

    /// Drain host/service completions and settle the resulting UI frame.
    pub fn poll_async(&mut self) -> Result<bool, Error> {
        let changed = self.source.poll_async();
        if changed {
            self.frame()?;
        }
        Ok(changed)
    }

    /// Drain one native host action emitted by the application under test.
    pub fn take_host_action(&mut self) -> Option<plico_renderer::HostAction> {
        self.source.take_host_action()
    }

    /// Theme used by the latest settled declaration.
    pub fn theme(&self) -> plico::Theme {
        self.source.theme()
    }

    /// Whether the real runtime currently requests further animation frames.
    pub fn has_animation(&self) -> bool {
        self.source.has_anim()
    }

    /// Earliest deterministic repaint requested by motion or deferred UI work.
    pub fn animation_deadline(&self) -> Option<web_time::Instant> {
        self.source.animation_deadline()
    }

    /// Shader validation messages produced by the latest settled frame.
    pub fn shader_diagnostics(&self) -> impl Iterator<Item = &str> {
        self.source.shader_diagnostics()
    }

    /// Deliver a backend-neutral event and settle the resulting frame.
    pub fn dispatch_event(&mut self, event: UiEvent) -> Result<EventResponse, Error> {
        self.source
            .try_prepare_for_event(&mut self.text)
            .map_err(|error| Error::Declaration {
                message: error.to_string(),
            })?;
        let response = self.source.handle_event(event);
        self.frame()?;
        Ok(response)
    }
    pub fn execute(&mut self, command: Command) -> Result<SemanticSnapshot, Error> {
        dispatch(&mut self.source, &mut self.text, &command)?;
        self.frame()?;
        Ok(self.snapshot())
    }
    pub fn snapshot(&self) -> SemanticSnapshot {
        self.source.semantics()
    }
    /// Find one exposed node by semantic role and accessible name. Repeated
    /// names are rejected so tests cannot silently select the wrong control.
    pub fn get_by_role(
        &self,
        role: SemanticRole,
        label: &str,
    ) -> Result<plico_renderer::SemanticNode, Error> {
        self.get_by_role_at(role, label, None)
    }

    /// Find a specific occurrence when repeated accessible names are intentional.
    pub fn get_by_role_at(
        &self,
        role: SemanticRole,
        label: &str,
        occurrence: Option<usize>,
    ) -> Result<plico_renderer::SemanticNode, Error> {
        let tree = self.snapshot();
        let matches = tree
            .exposed_nodes()
            .into_iter()
            .filter(|node| node.role == role && node.label.as_deref() == Some(label))
            .collect::<Vec<_>>();
        let node = match occurrence {
            Some(index) => matches.get(index).copied(),
            None if matches.len() == 1 => matches.first().copied(),
            _ => None,
        };
        node.cloned().ok_or_else(|| Error::Target {
            role: role.as_str().into(),
            label: label.into(),
            matches: matches.len(),
        })
    }

    /// All exposed nodes of a role, in semantic source order.
    pub fn all_by_role(&self, role: SemanticRole) -> Vec<plico_renderer::SemanticNode> {
        self.snapshot()
            .exposed_nodes()
            .into_iter()
            .filter(|node| node.role == role)
            .cloned()
            .collect()
    }

    /// Interactive controls missing an accessible name. This is intentionally
    /// an audit rather than a panic so callers can produce their own report.
    pub fn unnamed_controls(&self) -> Vec<plico_renderer::SemanticNode> {
        self.snapshot()
            .exposed_nodes()
            .into_iter()
            .filter(|node| {
                matches!(
                    node.role,
                    SemanticRole::Button
                        | SemanticRole::TextInput
                        | SemanticRole::Link
                        | SemanticRole::CheckBox
                        | SemanticRole::RadioButton
                        | SemanticRole::Switch
                        | SemanticRole::ComboBox
                        | SemanticRole::MenuItem
                        | SemanticRole::MenuItemCheckBox
                        | SemanticRole::MenuItemRadio
                        | SemanticRole::Slider
                        | SemanticRole::SpinButton
                        | SemanticRole::Tab
                        | SemanticRole::TreeItem
                        | SemanticRole::ColorWell
                ) && node
                    .label
                    .as_deref()
                    .is_none_or(|label| label.trim().is_empty())
            })
            .cloned()
            .collect()
    }

    /// Whether the runtime is asking the host for another immediate frame.
    pub fn frame_pending(&self) -> bool {
        self.has_animation()
    }

    /// Advance deterministic 60 Hz frames until active motion finishes.
    pub fn settle(&mut self) -> Result<SemanticSnapshot, Error> {
        self.settle_for(120, Duration::from_nanos(16_666_667))
    }

    /// Configurable variant for long animations and precise test frame rates.
    pub fn settle_for(
        &mut self,
        max_frames: usize,
        frame_interval: Duration,
    ) -> Result<SemanticSnapshot, Error> {
        for _ in 0..max_frames {
            if !self.frame_pending() {
                return Ok(self.snapshot());
            }
            self.advance(frame_interval)?;
        }
        if self.frame_pending() {
            Err(Error::DidNotSettle { frames: max_frames })
        } else {
            Ok(self.snapshot())
        }
    }
    pub fn declarations(&self) -> Vec<ViewNode> {
        self.source.engine.snapshot()
    }
    pub fn advance(&mut self, duration: Duration) -> Result<SemanticSnapshot, Error> {
        self.clock.advance(duration).map_err(|_| Error::Clock)?;
        self.frame()?;
        Ok(self.snapshot())
    }
    pub fn resize(&mut self, width: u32, height: u32) -> Result<SemanticSnapshot, Error> {
        if width == 0 || height == 0 {
            return Err(Error::Viewport);
        }
        self.size = (width, height);
        self.frame()?;
        Ok(self.snapshot())
    }
}
