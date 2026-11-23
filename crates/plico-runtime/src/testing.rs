//! Deterministic, renderer-independent application testing.
//!
//! Commands can be replayed from JSON or sent directly from Rust. No window,
//! GPU, OS focus, pixel coordinates or sleeps are needed.
use crate::{HostView, Source};
use plico::{ViewNode, VirtualClock};
use plico_renderer::{
    EventResponse, FrameSource, KeyEvent, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, PointerProperties, Scene, SemanticAction, SemanticNode, SemanticRole,
    SemanticSnapshot, TextContext, UiEvent, layout::PlacedNode,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::time::Duration;

fn is_control_role(role: SemanticRole) -> bool {
    matches!(
        role,
        SemanticRole::Button
            | SemanticRole::TextInput
            | SemanticRole::Link
            | SemanticRole::CheckBox
            | SemanticRole::RadioButton
            | SemanticRole::Switch
            | SemanticRole::ComboBox
            | SemanticRole::Option
            | SemanticRole::MenuItem
            | SemanticRole::MenuItemCheckBox
            | SemanticRole::MenuItemRadio
            | SemanticRole::TreeItem
            | SemanticRole::Separator
            | SemanticRole::Slider
            | SemanticRole::SpinButton
            | SemanticRole::Tab
            | SemanticRole::GridCell
            | SemanticRole::ColorWell
    )
}

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
    #[snafu(display("{contract} failed at frame {frame} (+{elapsed:?}): {detail}"))]
    FrameContract {
        contract: String,
        frame: usize,
        elapsed: Duration,
        detail: String,
    },
    #[snafu(display("virtual clock overflow"))]
    Clock,
    #[snafu(display("incremental/full-replay mismatch in {surface}: {detail}"))]
    Mismatch { surface: String, detail: String },
}

fn find_target<'a>(
    tree: &'a SemanticSnapshot,
    role: &str,
    label: &str,
    occurrence: Option<usize>,
) -> Result<&'a SemanticNode, Error> {
    let role =
        SemanticRole::from_name(role).ok_or_else(|| Error::UnknownRole { role: role.into() })?;
    find_by_role(tree, role, label, occurrence)
}

fn find_by_role<'a>(
    tree: &'a SemanticSnapshot,
    role: SemanticRole,
    label: &str,
    occurrence: Option<usize>,
) -> Result<&'a SemanticNode, Error> {
    tree.node_by_role(role, label, occurrence)
        .ok_or_else(|| Error::Target {
            role: role.as_str().into(),
            label: label.into(),
            matches: tree
                .exposed_nodes()
                .into_iter()
                .filter(|node| node.role == role && node.label.as_deref() == Some(label))
                .count(),
        })
}

fn target_point<V: HostView>(source: &Source<V>, node: &SemanticNode) -> Result<Point, Error> {
    let bounds = node.bounds;
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
    if hit != Some(node.id) {
        return Err(Error::PointerTarget {
            role: node.role.as_str().into(),
            label: node.label.clone().unwrap_or_default(),
            x: position.x,
            y: position.y,
            hit,
        });
    }
    Ok(position)
}

fn pointer_click<V: HostView>(
    source: &mut Source<V>,
    role: &str,
    label: &str,
    occurrence: Option<usize>,
    button: PointerButton,
) -> Result<(), Error> {
    let tree = source.shared_semantics();
    let target = find_target(&tree, role, label, occurrence)?;
    let position = target_point(source, target)?;
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
            let target = find_target(&source.shared_semantics(), role, label, *occurrence)?.id;
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
            let target = find_target(&source.shared_semantics(), "slider", label, None)?.id;
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

/// MyGO-style name for the deterministic high-level test driver.
///
/// `Driver` remains available for recipe/transport-oriented code. Application
/// tests can use `Tester` and the concise name-based helpers on the same type.
pub type Tester<V> = Driver<V>;

/// Stable layout projection used by differential tests.
///
/// Renderer resource handles and cache identities are deliberately absent.
/// Scene commands are compared separately through [`SceneSnapshot`].
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSnapshot {
    pub depth: usize,
    pub transform: [f64; 6],
    pub opacity: f32,
    pub rect: [f32; 4],
    pub content_origin: [f32; 2],
    pub content_size: [f32; 2],
    pub clip: Option<[f32; 4]>,
    pub clip_radii: [f32; 4],
    pub own_clip: Option<[f32; 4]>,
    pub own_clip_radii: [f32; 4],
    pub border_widths: [f32; 4],
    pub scroll: plico_renderer::layout::ScrollMetrics,
}

impl From<&PlacedNode> for LayoutSnapshot {
    fn from(node: &PlacedNode) -> Self {
        let radii = |radii: plico_renderer::style::CornerRadii| {
            [
                radii.top_left,
                radii.top_right,
                radii.bottom_right,
                radii.bottom_left,
            ]
        };
        Self {
            depth: node.depth,
            transform: node.world_transform.as_coeffs(),
            opacity: node.world_opacity,
            rect: node.rect,
            content_origin: node.content_origin,
            content_size: node.content_size,
            clip: node.clip,
            clip_radii: radii(node.clip_radii),
            own_clip: node.own_clip,
            own_clip_radii: radii(node.own_clip_radii),
            border_widths: node.border_widths,
            scroll: node.scroll,
        }
    }
}

/// Pointer-independent logical signature of a rendered scene.
///
/// `PaintCommand` debug output intentionally omits allocation addresses and
/// opaque SVG internals, while retaining command order, geometry and paint
/// parameters. Pixel comparisons remain the stronger GPU-specific oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneSnapshot {
    pub commands: Vec<String>,
}

#[derive(Clone)]
enum ReplayStep {
    Execute(Command),
    Advance(Duration),
    Resize(u32, u32),
    Settle,
}

/// Compare a long-lived incremental runtime with a cold runtime that replays
/// the same deterministic operations from the initial application state.
///
/// This catches stale declaration, layout, semantic, hit-test and Scene
/// caches without requiring the application to serialize its state. Arbitrary
/// `update_view` closures are intentionally excluded because they cannot be
/// replayed; express those changes as UI commands or application-owned test
/// actions instead.
pub struct DifferentialTester<V: HostView + Clone> {
    initial: V,
    initial_size: (u32, u32),
    live: Driver<V>,
    steps: Vec<ReplayStep>,
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
    /// Build and settle one deterministic frame at the current virtual time.
    pub fn frame(&mut self) -> Result<(), Error> {
        self.source
            .prepare_frame(&mut self.text, self.size, true)
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

    /// Stable geometry and scrolling projection for cache-oracle comparisons.
    pub fn layout_snapshot(&self) -> Vec<LayoutSnapshot> {
        self.placed_nodes()
            .iter()
            .map(LayoutSnapshot::from)
            .collect()
    }

    /// Encode the retained frame without forcing another declaration and
    /// return a resource-pointer-independent logical command signature.
    pub fn scene_snapshot(&mut self) -> Result<SceneSnapshot, Error> {
        let mut scene = Scene::new();
        self.source
            .try_render_retained_frame(&mut scene, &mut self.text, self.size, 1.)
            .map_err(|error| Error::Declaration {
                message: error.to_string(),
            })?;
        Ok(SceneSnapshot {
            commands: scene
                .commands
                .iter()
                .map(|command| normalize_scene_debug(format!("{command:?}")))
                .collect(),
        })
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
        self.apply_command(&command)?;
        Ok(self.snapshot())
    }
    fn apply_command(&mut self, command: &Command) -> Result<(), Error> {
        dispatch(&mut self.source, &mut self.text, command)?;
        self.frame()
    }
    pub fn snapshot(&self) -> SemanticSnapshot {
        self.source.semantics()
    }

    /// Find one exposed element by accessible name, independent of its role.
    ///
    /// Names must be unique. Use [`Self::find_at`] when repeated names are
    /// intentional, or [`Self::get_by_role`] when the role is part of the
    /// assertion.
    pub fn find(&self, label: &str) -> Result<plico_renderer::SemanticNode, Error> {
        self.find_at(label, None)
    }

    /// Find a zero-based occurrence of an exposed accessible name.
    pub fn find_at(
        &self,
        label: &str,
        occurrence: Option<usize>,
    ) -> Result<plico_renderer::SemanticNode, Error> {
        let tree = self.source.shared_semantics();
        let mut matches = tree
            .exposed_nodes()
            .into_iter()
            .filter(|node| node.label.as_deref() == Some(label))
            .collect::<Vec<_>>();
        // A named control commonly owns a static-text child with the same
        // accessible name. MyGO's Tester resolves that visible text to the
        // receiver of input; do the same while preserving true ambiguity
        // between two independently named controls.
        if matches.iter().any(|node| is_control_role(node.role)) {
            matches.retain(|node| is_control_role(node.role));
        }
        let node = match occurrence {
            Some(index) => matches.get(index).copied(),
            None if matches.len() == 1 => matches.first().copied(),
            _ => None,
        };
        node.cloned().ok_or_else(|| Error::Target {
            role: "element".into(),
            label: label.into(),
            matches: matches.len(),
        })
    }

    /// Accessible names in exposed semantic source order.
    pub fn texts(&self) -> Vec<String> {
        self.source
            .shared_semantics()
            .exposed_nodes()
            .into_iter()
            .filter_map(|node| node.label.clone())
            .collect()
    }

    /// Whether any exposed accessible name or textual value contains `text`.
    pub fn has_text(&self, text: &str) -> bool {
        self.source
            .shared_semantics()
            .exposed_nodes()
            .into_iter()
            .any(|node| {
                node.label
                    .as_deref()
                    .is_some_and(|label| label.contains(text))
                    || node
                        .value
                        .as_deref()
                        .is_some_and(|value| value.contains(text))
            })
    }

    /// Move the primary mouse pointer in logical window coordinates.
    pub fn move_pointer(&mut self, x: f64, y: f64) -> Result<SemanticSnapshot, Error> {
        self.execute(Command::PointerMove { x, y })
    }

    /// Move the pointer to the center of a uniquely named, hit-testable element.
    pub fn move_to(&mut self, label: &str) -> Result<SemanticSnapshot, Error> {
        let target = self.find(label)?;
        let point = target_point(&self.source, &target)?;
        self.move_pointer(point.x, point.y)
    }

    /// Press the primary pointer without releasing it.
    pub fn press_at(&mut self, x: f64, y: f64) -> Result<SemanticSnapshot, Error> {
        self.execute(Command::PointerDown { x, y })
    }

    /// Release the primary pointer.
    pub fn release_at(&mut self, x: f64, y: f64) -> Result<SemanticSnapshot, Error> {
        self.execute(Command::PointerUp { x, y })
    }

    /// Click the center of a uniquely named element through real hit testing.
    pub fn click(&mut self, label: &str) -> Result<SemanticSnapshot, Error> {
        let target = self.find(label)?;
        let point = target_point(&self.source, &target)?;
        self.apply_command(&Command::PointerMove {
            x: point.x,
            y: point.y,
        })?;
        self.apply_command(&Command::PointerDown {
            x: point.x,
            y: point.y,
        })?;
        self.release_at(point.x, point.y)
    }

    /// Drag a uniquely named element by a logical-pixel delta.
    pub fn drag_by(
        &mut self,
        label: &str,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<SemanticSnapshot, Error> {
        if !delta_x.is_finite() || !delta_y.is_finite() {
            return Err(Error::PointerPosition);
        }
        let target = self.find(label)?;
        let start = target_point(&self.source, &target)?;
        let end = Point {
            x: start.x + delta_x,
            y: start.y + delta_y,
        };
        self.apply_command(&Command::PointerMove {
            x: start.x,
            y: start.y,
        })?;
        self.apply_command(&Command::PointerDown {
            x: start.x,
            y: start.y,
        })?;
        self.apply_command(&Command::PointerMove { x: end.x, y: end.y })?;
        self.release_at(end.x, end.y)
    }

    /// Commit text to the currently focused input.
    pub fn type_text(&mut self, text: impl Into<String>) -> Result<SemanticSnapshot, Error> {
        self.execute(Command::Text { text: text.into() })
    }

    /// Send one logical key press to the current focus owner.
    pub fn key(&mut self, key: impl Into<String>) -> Result<SemanticSnapshot, Error> {
        self.execute(Command::Key { key: key.into() })
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
        find_by_role(&self.source.shared_semantics(), role, label, occurrence).cloned()
    }

    /// All exposed nodes of a role, in semantic source order.
    pub fn all_by_role(&self, role: SemanticRole) -> Vec<plico_renderer::SemanticNode> {
        self.source
            .shared_semantics()
            .exposed_nodes()
            .into_iter()
            .filter(|node| node.role == role)
            .cloned()
            .collect()
    }

    /// Interactive controls missing an accessible name. This is intentionally
    /// an audit rather than a panic so callers can produce their own report.
    pub fn unnamed_controls(&self) -> Vec<plico_renderer::SemanticNode> {
        self.source
            .shared_semantics()
            .exposed_nodes()
            .into_iter()
            .filter(|node| {
                is_control_role(node.role)
                    && node
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

    /// Check a transition against each consecutive pair of presented semantic
    /// snapshots. The first pair starts at the current frame, immediately after
    /// the triggering action. Failure leaves the driver at the offending frame.
    pub fn check_frames(
        &mut self,
        contract: &str,
        frames: usize,
        interval: Duration,
        mut check: impl FnMut(&SemanticSnapshot, &SemanticSnapshot) -> Result<(), String>,
    ) -> Result<(), Error> {
        let mut previous = self.source.shared_semantics();
        let mut elapsed = Duration::ZERO;
        for frame in 1..=frames {
            self.advance_frame(interval)?;
            let current = self.source.shared_semantics();
            elapsed += interval;
            check(&previous, &current).map_err(|detail| Error::FrameContract {
                contract: contract.into(),
                frame,
                elapsed,
                detail,
            })?;
            previous = current;
        }
        Ok(())
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
            self.advance_frame(frame_interval)?;
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
        self.advance_frame(duration)?;
        Ok(self.snapshot())
    }
    fn advance_frame(&mut self, duration: Duration) -> Result<(), Error> {
        self.clock.advance(duration).map_err(|_| Error::Clock)?;
        self.frame()
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

impl<V: HostView + Clone> DifferentialTester<V> {
    pub fn new(view: V, width: u32, height: u32) -> Result<Self, Error> {
        let mut verifier = Self {
            initial: view.clone(),
            initial_size: (width, height),
            live: Driver::new(view, width, height)?,
            steps: Vec::new(),
        };
        verifier.verify()?;
        Ok(verifier)
    }

    /// Current application state from the long-lived incremental runtime.
    pub fn view(&self) -> &V {
        self.live.view()
    }

    pub fn snapshot(&self) -> SemanticSnapshot {
        self.live.snapshot()
    }

    pub fn declarations(&self) -> Vec<ViewNode> {
        self.live.declarations()
    }

    /// Execute one serializable recipe command, then compare every retained
    /// projection with a freshly replayed runtime.
    pub fn execute(&mut self, command: Command) -> Result<SemanticSnapshot, Error> {
        self.live.apply_command(&command)?;
        self.steps.push(ReplayStep::Execute(command));
        self.verify()?;
        Ok(self.snapshot())
    }

    pub fn advance(&mut self, duration: Duration) -> Result<SemanticSnapshot, Error> {
        self.live.advance_frame(duration)?;
        self.steps.push(ReplayStep::Advance(duration));
        self.verify()?;
        Ok(self.snapshot())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<SemanticSnapshot, Error> {
        self.live.resize(width, height)?;
        self.steps.push(ReplayStep::Resize(width, height));
        self.verify()?;
        Ok(self.snapshot())
    }

    pub fn settle(&mut self) -> Result<SemanticSnapshot, Error> {
        self.live.settle()?;
        self.steps.push(ReplayStep::Settle);
        self.verify()?;
        Ok(self.snapshot())
    }

    fn replay(&self) -> Result<Driver<V>, Error> {
        let mut oracle = Driver::new(
            self.initial.clone(),
            self.initial_size.0,
            self.initial_size.1,
        )?;
        for step in &self.steps {
            match step {
                ReplayStep::Execute(command) => {
                    oracle.apply_command(command)?;
                }
                ReplayStep::Advance(duration) => {
                    oracle.advance_frame(*duration)?;
                }
                ReplayStep::Resize(width, height) => {
                    oracle.resize(*width, *height)?;
                }
                ReplayStep::Settle => {
                    oracle.settle()?;
                }
            }
        }
        Ok(oracle)
    }

    /// Rebuild the oracle from the initial state and compare declarations,
    /// semantics, layout, hit testing and logical Scene commands.
    pub fn verify(&mut self) -> Result<(), Error> {
        let mut oracle = self.replay()?;

        let live_declarations = self.live.declarations();
        let oracle_declarations = oracle.declarations();
        if live_declarations != oracle_declarations {
            return Err(Error::Mismatch {
                surface: "declarations".into(),
                detail: sequence_difference(&live_declarations, &oracle_declarations),
            });
        }

        let live_semantics = self.live.source.shared_semantics();
        let oracle_semantics = oracle.source.shared_semantics();
        if live_semantics != oracle_semantics {
            return Err(Error::Mismatch {
                surface: "semantics".into(),
                detail: sequence_difference(&live_semantics.nodes, &oracle_semantics.nodes),
            });
        }

        let live_layout = self.live.layout_snapshot();
        let oracle_layout = oracle.layout_snapshot();
        if live_layout != oracle_layout {
            return Err(Error::Mismatch {
                surface: "layout".into(),
                detail: sequence_difference(&live_layout, &oracle_layout),
            });
        }

        let live_scene = self.live.scene_snapshot()?;
        let oracle_scene = oracle.scene_snapshot()?;
        if live_scene != oracle_scene {
            return Err(Error::Mismatch {
                surface: "scene".into(),
                detail: sequence_difference(&live_scene.commands, &oracle_scene.commands),
            });
        }

        let live_hits = hit_snapshot(&self.live);
        let oracle_hits = hit_snapshot(&oracle);
        if live_hits != oracle_hits {
            return Err(Error::Mismatch {
                surface: "hit-test".into(),
                detail: sequence_difference(&live_hits, &oracle_hits),
            });
        }
        Ok(())
    }
}

fn sequence_difference<T: PartialEq + std::fmt::Debug>(live: &[T], oracle: &[T]) -> String {
    let first = live
        .iter()
        .zip(oracle)
        .position(|(live, oracle)| live != oracle);
    match first {
        Some(index) => {
            let live_value = format!("{:?}", live[index]);
            let oracle_value = format!("{:?}", oracle[index]);
            let shorten = |value: String| {
                const LIMIT: usize = 500;
                if value.len() <= LIMIT {
                    value
                } else {
                    format!("{}…", &value[..value.floor_char_boundary(LIMIT)])
                }
            };
            format!(
                "first difference at index {index}; incremental={}, replay={}; live={}; oracle={}",
                live.len(),
                oracle.len(),
                shorten(live_value),
                shorten(oracle_value)
            )
        }
        None => format!(
            "length differs; incremental={}, replay={}",
            live.len(),
            oracle.len()
        ),
    }
}

fn hit_snapshot<V: HostView>(driver: &Driver<V>) -> Vec<Option<u64>> {
    let mut points = Vec::new();
    let (width, height) = driver.size;
    for y in 0..=4 {
        for x in 0..=4 {
            points.push(Point {
                x: f64::from(width) * (f64::from(x) + 0.5) / 5.,
                y: f64::from(height) * (f64::from(y) + 0.5) / 5.,
            });
        }
    }
    points.extend(
        driver
            .source
            .shared_semantics()
            .exposed_nodes()
            .into_iter()
            .filter_map(|node| {
                let [x0, y0, x1, y1] = node.bounds;
                (x0.is_finite()
                    && y0.is_finite()
                    && x1.is_finite()
                    && y1.is_finite()
                    && x1 > x0
                    && y1 > y0)
                    .then_some(Point {
                        x: f64::from(x0 + x1) * 0.5,
                        y: f64::from(y0 + y1) * 0.5,
                    })
            }),
    );
    points
        .into_iter()
        .map(|point| driver.source.hit_test(point).map(plico::Entity::raw))
        .collect()
}

fn normalize_scene_debug(mut command: String) -> String {
    // FontData's Blob ID is a process-global cache identity, not part of the
    // rendered command. Two fresh TextContexts intentionally assign different
    // IDs to identical font bytes.
    const PREFIX: &str = "Blob { id: ";
    let mut offset = 0;
    while let Some(found) = command[offset..].find(PREFIX) {
        let start = offset + found + PREFIX.len();
        let end = command[start..]
            .find(|character: char| !character.is_ascii_digit())
            .map_or(command.len(), |end| start + end);
        command.replace_range(start..end, "_");
        offset = start + 1;
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_frames_settle_layout_without_cloning_placements() {
        struct Label;
        impl HostView for Label {
            fn show(&mut self, cx: &mut plico::UiContext) {
                cx.text("Ready");
            }
        }
        let mut driver = Driver::new(Label, 320, 240).unwrap();
        assert!(driver.has_text("Ready"));
        assert!(!driver.placed_nodes().is_empty());
        assert_eq!(
            driver.source.frame_diagnostics().backend.placed_node_clones,
            0
        );
        driver.advance(Duration::from_millis(16)).unwrap();
        assert_eq!(
            driver.source.frame_diagnostics().backend.placed_node_clones,
            0
        );
    }
}
