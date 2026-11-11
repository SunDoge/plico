//! Backend-neutral window, input, host-action, and frame diagnostic contracts.

use std::{ops::Range, path::PathBuf, sync::Arc};

pub use crate::platform::accessibility::{
    SemanticAction, SemanticCurrent, SemanticNode, SemanticPopup, SemanticRole, SemanticSnapshot,
    SemanticSortDirection, SemanticStates, SemanticToggleState,
};

/// Logical pixel delta emitted for one discrete mouse-wheel line.
///
/// Consumers with line-based scrolling should accumulate pixel deltas against
/// this value so high-resolution trackpads and discrete wheels share one unit.
pub const WHEEL_LINE_DELTA: f64 = 40.0;
/// Host multi-click policy in logical window coordinates. Time is compared
/// against the UI's injected clock; this value never reads a system clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultiClickSettings {
    /// Maximum time between successive presses.
    pub interval: std::time::Duration,
    /// Maximum radial movement between presses, in logical pixels.
    pub distance: f64,
}
impl Default for MultiClickSettings {
    fn default() -> Self {
        Self {
            interval: std::time::Duration::from_millis(500),
            distance: 4.,
        }
    }
}
impl MultiClickSettings {
    /// Replace invalid spatial thresholds with the portable default.
    pub fn normalized(self) -> Self {
        if self.distance.is_finite() && self.distance >= 0. {
            self
        } else {
            Self {
                distance: Self::default().distance,
                ..self
            }
        }
    }
    /// Check time and distance; callers retain button, pointer and target checks.
    pub fn accepts(self, previous: Point, current: Point, elapsed: std::time::Duration) -> bool {
        let settings = self.normalized();
        elapsed <= settings.interval
            && (current.x - previous.x).hypot(current.y - previous.y) <= settings.distance
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Current native window state delivered to a frame source.
pub struct WindowMetrics {
    /// Stable Wabou window identifier, independent of platform handles.
    pub window_key: crate::platform::WindowResourceKey,
    /// Logical client-area width.
    pub logical_width: u32,
    /// Logical client-area height.
    pub logical_height: u32,
    /// Physical surface width.
    pub physical_width: u32,
    /// Physical surface height.
    pub physical_height: u32,
    /// Physical pixels per logical pixel.
    pub scale_factor: f64,
    /// Whether the native window is maximized.
    pub maximized: bool,
    /// Whether the native window is fullscreen.
    pub fullscreen: bool,
    /// Whether the native window owns keyboard focus.
    pub focused: bool,
    /// Outer-window horizontal desktop coordinate, when the platform exposes it.
    pub outer_x: Option<i32>,
    /// Outer-window vertical desktop coordinate, when the platform exposes it.
    pub outer_y: Option<i32>,
    /// Whether the compositor reports the window as completely hidden.
    pub occluded: bool,
    /// Current native light/dark preference, when reported by the platform.
    pub color_scheme: Option<ColorScheme>,
    /// Whether the platform requests reduced non-essential motion.
    pub reduced_motion: bool,
    /// Whether native materials should use an opaque fallback.
    pub reduced_transparency: bool,
    /// Whether semantic borders and secondary text should have more contrast.
    pub high_contrast: bool,
}
impl WindowMetrics {
    /// Project native display preferences into the renderer-free UI contract.
    pub fn display_preferences(self) -> plico::DisplayPreferences {
        plico::DisplayPreferences {
            reduce_motion: self.reduced_motion,
            reduce_transparency: self.reduced_transparency,
            high_contrast: self.high_contrast,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Platform color preference associated with a native window.
pub enum ColorScheme {
    /// Prefer a light application palette.
    Light,
    /// Prefer a dark application palette.
    Dark,
}

impl Default for WindowMetrics {
    fn default() -> Self {
        Self {
            window_key: crate::platform::initial_window_resource_key(0),
            logical_width: 0,
            logical_height: 0,
            physical_width: 0,
            physical_height: 0,
            scale_factor: 1.0,
            maximized: false,
            fullscreen: false,
            focused: false,
            outer_x: None,
            outer_y: None,
            occluded: false,
            color_scheme: Some(ColorScheme::Light),
            reduced_motion: false,
            reduced_transparency: false,
            high_contrast: false,
        }
    }
}

/// Wabou's renderer-independent input model. Web-style events are an adapter
/// concern of individual frame sources, not part of the shell contract.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    /// Horizontal logical coordinate.
    pub x: f64,
    /// Vertical logical coordinate.
    pub y: f64,
}

pub use plico::{KeyEvent, KeyLocation, KeyPhase, Modifiers, StandardShortcut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Pointer button independent of winit and DOM numbering.
pub enum PointerButton {
    /// Main activation button, normally left.
    Primary,
    /// Auxiliary button, normally middle.
    Auxiliary,
    /// Context-menu button, normally right.
    Secondary,
    /// Platform button not covered by the standard variants.
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Lifecycle phase of a pointer interaction.
pub enum PointerPhase {
    /// Pointer entered the native window.
    Enter,
    /// Pointer moved without changing button state.
    Move,
    /// A button was pressed.
    Down,
    /// A button was released.
    Up,
    /// The platform cancelled the active pointer sequence.
    Cancel,
    /// Pointer left the native window.
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
/// Stable identity of one active native pointer, represented as FFI-safe words.
pub struct PointerId {
    /// Low 32 bits.
    pub lo: u32,
    /// High 32 bits, including a source namespace.
    pub hi: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Native pointer category exposed without DOM compatibility aliases.
pub enum PointerType {
    /// Conventional mouse or mouse-compatible pointing device.
    Mouse,
    /// Direct touchscreen contact.
    Touch,
    /// Tablet pen, eraser, brush, or similar tool.
    Pen,
    /// Platform source which cannot be classified reliably.
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Device properties accompanying a pointer transition.
pub struct PointerProperties {
    /// Identity stable for the active pointer sequence.
    pub id: PointerId,
    /// Native pointer category.
    pub pointer_type: PointerType,
    /// Whether this is the primary pointer for its category.
    pub primary: bool,
    /// Normalized contact pressure, when reported by the platform.
    pub pressure: Option<f64>,
    /// Normalized tablet barrel pressure in the range -1 to 1.
    pub tangential_pressure: Option<f64>,
    /// Tablet tilt around the surface Y-Z plane, in degrees.
    pub tilt_x: Option<f64>,
    /// Tablet tilt around the surface X-Z plane, in degrees.
    pub tilt_y: Option<f64>,
    /// Clockwise tablet-tool rotation in degrees.
    pub twist: Option<f64>,
}

impl Default for PointerProperties {
    fn default() -> Self {
        Self {
            id: PointerId { lo: 1, hi: 0 },
            pointer_type: PointerType::Mouse,
            primary: true,
            pressure: None,
            tangential_pressure: None,
            tilt_x: None,
            tilt_y: None,
            twist: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Pointer event in logical window or widget-local coordinates.
pub struct PointerEvent {
    /// Interaction phase.
    pub phase: PointerPhase,
    /// Pointer position in the receiver's documented coordinate space.
    pub position: Point,
    /// Button changed by this event, if any.
    pub button: Option<PointerButton>,
    /// DOM-compatible bit layout is deliberately not required here. This is a
    /// native set owned by the shell; adapters translate it at their boundary.
    pub buttons: u32,
    /// Keyboard modifiers active for the event.
    pub modifiers: Modifiers,
    /// Pointer identity, category, and optional contact/tool measurements.
    pub properties: PointerProperties,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Pixel-normalized wheel or trackpad event.
pub struct WheelEvent {
    /// Pointer position in the receiver's documented coordinate space.
    pub position: Point,
    /// Horizontal logical-pixel delta.
    pub delta_x: f64,
    /// Vertical logical-pixel delta.
    pub delta_y: f64,
    /// Whether the native source reported discrete lines or precise pixels.
    pub delta_mode: WheelDeltaMode,
    /// Native scroll lifecycle phase.
    pub phase: GesturePhase,
    /// Keyboard modifiers active for the event.
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Native granularity of a wheel delta.
pub enum WheelDeltaMode {
    /// A discrete mouse-wheel notch, normalized to logical pixels by the shell.
    Line,
    /// A precise trackpad or high-resolution wheel delta in logical pixels.
    Pixel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Lifecycle phase shared by native continuous gestures.
pub enum GesturePhase {
    /// The gesture has started.
    Started,
    /// The gesture changed since its previous update.
    Changed,
    /// The gesture ended normally.
    Ended,
    /// The platform cancelled the gesture.
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Native trackpad or touchscreen gesture kept independent from DOM events.
pub enum GestureEvent {
    /// Relative magnification; positive values zoom in.
    Pinch {
        /// Relative magnification since the previous update.
        delta: f64,
        /// Current gesture lifecycle phase.
        phase: GesturePhase,
    },
    /// Relative translation in logical window pixels.
    Pan {
        /// Horizontal logical-pixel change.
        delta_x: f64,
        /// Vertical logical-pixel change.
        delta_y: f64,
        /// Current gesture lifecycle phase.
        phase: GesturePhase,
    },
    /// Relative counter-clockwise rotation in degrees.
    Rotation {
        /// Relative counter-clockwise rotation in degrees.
        delta: f64,
        /// Current gesture lifecycle phase.
        phase: GesturePhase,
    },
    /// Platform smart-zoom/double-tap gesture.
    DoubleTap,
    /// Force-touch pressure and platform click stage.
    Pressure {
        /// Normalized force in the inclusive range zero to one.
        pressure: f64,
        /// Platform click stage.
        stage: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Operating-system application lifecycle notification.
pub enum AppLifecycleEvent {
    /// The application became active and may resume foreground work.
    Resumed,
    /// The application should pause foreground work.
    Suspended,
    /// The operating system requested that caches be reduced promptly.
    MemoryWarning,
}

/// Platform input-method lifecycle delivered to the focused native widget.
/// Cursor offsets and delete ranges are UTF-8 byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEvent {
    /// Platform text input was enabled for the focused control.
    Enabled,
    /// Replace the active preedit text without committing it.
    Preedit {
        /// Current preedit string.
        text: String,
        /// Selected byte range within `text`, if supplied by the IME.
        cursor: Option<(usize, usize)>,
    },
    /// Commit text and finish the active composition.
    Commit(String),
    /// Delete UTF-8 bytes around the insertion point before the next update.
    DeleteSurrounding {
        /// Bytes immediately before the insertion point.
        before_bytes: usize,
        /// Bytes immediately after the insertion point.
        after_bytes: usize,
    },
    /// Platform text input was disabled for the control.
    Disabled,
}

/// Focused text state exposed by a backend-owned editor to the native IME.
///
/// Selection offsets use UTF-16 because that is the common contract expected
/// by macOS and native input handlers. Backends remain responsible for mapping
/// those offsets to their authoritative UTF-8 document representation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectedTextInputState {
    /// Whether the focused Wabou node currently accepts text input.
    pub accepts_text: bool,
    /// Controlled text value, when exposed by the focused widget.
    pub text: Option<String>,
    /// UTF-16 selection range in document order.
    pub selection: Option<Range<usize>>,
    /// Whether the selection's moving head precedes its anchor.
    pub selection_reversed: bool,
    /// Candidate-window anchor in window-logical coordinates.
    pub cursor_bounds: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Phase of a file drag entering, leaving, or being dropped on a window.
pub enum FileDropPhase {
    /// One or more native files entered the window.
    Entered,
    /// A native file drag moved within the window.
    Moved,
    /// The native file drag left the window without dropping.
    Left,
    /// A native file was dropped on the window.
    Dropped,
}

#[derive(Debug, Clone, PartialEq)]
/// Window-level native file drag-and-drop event.
pub struct FileDropEvent {
    /// Drag lifecycle phase reported by the window system.
    pub phase: FileDropPhase,
    /// Native paths supplied when the drag enters or drops.
    pub paths: Vec<std::path::PathBuf>,
    /// Pointer position in logical window pixels, when supplied by the platform.
    pub position: Option<Point>,
}

#[derive(Debug, Clone, PartialEq)]
/// Input and window-state events delivered to a frame source or widget.
pub enum UiEvent {
    /// Operating-system application lifecycle notification.
    AppLifecycle(AppLifecycleEvent),
    /// Authoritative physical modifier-key state changed.
    ModifiersChanged(Modifiers),
    /// Pointer movement or button transition.
    Pointer(PointerEvent),
    /// Wheel or trackpad scrolling.
    Wheel(WheelEvent),
    /// Native trackpad or touchscreen gesture.
    Gesture(GestureEvent),
    /// Physical/logical keyboard transition.
    Key(KeyEvent),
    /// Text committed outside an IME composition.
    TextInput(String),
    /// Input-method lifecycle event.
    Ime(ImeEvent),
    /// Text returned by an asynchronous clipboard read.
    Paste(String),
    /// Element-level focus transition.
    Focus(bool),
    /// A native file was dragged over, away from, or dropped on the window.
    FileDrop(FileDropEvent),
    /// Native window size, scale, or focus state changed.
    WindowMetrics(WindowMetrics),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Synchronous effects requested while handling a [`UiEvent`].
pub struct EventResponse {
    /// Whether the source consumed the event.
    pub handled: bool,
    /// Whether the next frame may differ and should be scheduled.
    pub request_redraw: bool,
    /// Suppress the committed text paired with the current physical key.
    /// This is intentionally independent from `handled`: JS key listeners
    /// must not accidentally swallow text input.
    pub consume_key_text: bool,
    /// `Some(true)` enables platform text input/IME; `Some(false)` disables it.
    /// `None` leaves the current window setting unchanged.
    pub text_input: Option<bool>,
    /// Clipboard operation to execute after event dispatch.
    pub clipboard: Option<ClipboardRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Clipboard operation requested synchronously by event handling.
pub enum ClipboardRequest {
    /// Replace clipboard text.
    Write(String),
    /// Read text and later deliver it as [`UiEvent::Paste`].
    Read,
}

/// An asynchronous request from a frame source to its native window host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostAction {
    /// Control the current native window, without needing a window resource ID.
    Window(crate::WindowCommand),
    /// Open a URL with the platform's registered handler.
    OpenUrl(String),
    /// Replace clipboard text without requiring an acknowledgement.
    ///
    /// Widget-side request vocabulary; the applier translates it into the
    /// clipboard-write effect before it reaches the shell.
    SetClipboard(String),
    /// Read clipboard text and route completion back to the producer.
    ///
    /// Widget-side request vocabulary; the applier translates it into the
    /// clipboard-read effect and delivers the result through
    /// [`crate::Widget::deliver_clipboard_text`].
    ReadClipboard {
        /// Producer-local request identifier.
        request_id: u64,
    },
    /// `None` restores the host's default title.
    SetWindowTitle(Option<String>),
    /// Ask the window manager to draw the user's attention.
    RequestAttention,
}

/// Thread-safe callback used by asynchronous producers to wake the native
/// application loop.
pub type WakeCallback = Arc<dyn Fn() + Send + Sync>;

/// Per-frame render-stage timings, as an exponential moving average (so a
/// live overlay isn't dominated by a single slow frame). Reported to the host
/// each frame for a perf overlay
/// (`useHost().diagnostics.frameStats()`). All timing is captured at the app/shell seam.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameStats {
    /// Total frame-source construction time in milliseconds.
    pub build_frame_ms: f64,
    /// Guest JavaScript frame callback time in milliseconds.
    pub js_tick_ms: f64,
    /// Backend-neutral scene assembly time in milliseconds.
    pub scene_ms: f64,
    /// Renderer submission and presentation time in milliseconds, or zero when
    /// the active native toolkit does not expose a reliable completion time.
    pub present_ms: f64,
    /// CPU time building the renderer's instance stream.
    pub prepare_ms: f64,
    /// CPU time in instance and frame-uniform uploads.
    pub upload_ms: f64,
    /// CPU time recording render passes and blits.
    pub encode_ms: f64,
    /// CPU time submitting command buffers.
    pub submit_ms: f64,
    /// Queue submissions issued for the latest frame.
    pub submissions: u64,
    /// Command buffers in the latest queue submission.
    pub command_buffers: u64,
    /// Blocking GPU wait for offscreen readback; zero for window presentation.
    pub wait_ms: f64,
    /// Bytes uploaded for the instance stream and frame uniforms.
    pub upload_bytes: u64,
    /// Instances prepared across the root scene and child layers.
    pub instances: u64,
    /// Instanced scene draw calls, including damage-scissor replays.
    pub draw_calls: u64,
    /// GPU frame time when timestamp queries are enabled. `None` when the
    /// backend does not report it. Blended like the other timings; the first
    /// sample is kept at full weight.
    pub gpu_ms: Option<f64>,
    /// Actual fraction of the acquired target redrawn in the last successful
    /// frame, including buffer history. `None` means the backend does not
    /// report it. This is a raw sample, unlike the timing EMAs.
    pub damage_coverage: Option<f64>,
    /// Number of retained nodes in the frame.
    pub node_count: usize,
    /// Logical viewport width.
    pub viewport_w: u32,
    /// Logical viewport height.
    pub viewport_h: u32,
}

/// An atomically reserved screenshot artifact fulfilled through its open file.
pub struct ScreenshotRequest {
    /// Stable identity and user-visible location of the capture.
    pub path: PathBuf,
    /// Reserved destination handle; renderers must not reopen `path`.
    pub file: std::fs::File,
}

/// Backend-neutral renderer work for one frame, blended into [`FrameStats`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameBudgetSample {
    /// CPU time building the instance stream.
    pub prepare_ms: f64,
    /// CPU time in instance and frame-uniform uploads.
    pub upload_ms: f64,
    /// CPU time recording render passes and blits.
    pub encode_ms: f64,
    /// CPU time submitting command buffers.
    pub submit_ms: f64,
    /// Queue submissions issued for this frame.
    pub submissions: u64,
    /// Command buffers in this frame's queue submission.
    pub command_buffers: u64,
    /// Blocking GPU wait for offscreen readback; zero for window presentation.
    pub wait_ms: f64,
    /// GPU frame time, when the backend reports timestamps.
    pub gpu_ms: Option<f64>,
    /// Bytes uploaded for the instance stream and frame uniforms.
    pub upload_bytes: u64,
    /// Instances prepared across the root scene and child layers.
    pub instances: u64,
    /// Instanced scene draw calls, including damage-scissor replays.
    pub draw_calls: u64,
}

impl FrameStats {
    const ALPHA: f64 = 0.1;

    /// Blend the renderer's CPU stages and GPU time into their EMAs and store
    /// this frame's raw counts.
    pub fn apply_budget(&mut self, budget: FrameBudgetSample) {
        let prev = 1.0 - Self::ALPHA;
        self.prepare_ms = self.prepare_ms * prev + budget.prepare_ms * Self::ALPHA;
        self.upload_ms = self.upload_ms * prev + budget.upload_ms * Self::ALPHA;
        self.encode_ms = self.encode_ms * prev + budget.encode_ms * Self::ALPHA;
        self.submit_ms = self.submit_ms * prev + budget.submit_ms * Self::ALPHA;
        self.wait_ms = self.wait_ms * prev + budget.wait_ms * Self::ALPHA;
        self.gpu_ms = match (self.gpu_ms, budget.gpu_ms) {
            (Some(current), Some(sample)) => Some(current * prev + sample * Self::ALPHA),
            (None, sample) => sample,
            (current, None) => current,
        };
        self.upload_bytes = budget.upload_bytes;
        self.submissions = budget.submissions;
        self.command_buffers = budget.command_buffers;
        self.instances = budget.instances;
        self.draw_calls = budget.draw_calls;
    }

    /// Blend this frame's raw samples into the EMA and refresh the node count.
    pub fn update(
        &mut self,
        build_frame_ms: f64,
        scene_ms: f64,
        present_ms: f64,
        node_count: usize,
    ) {
        let prev = 1.0 - Self::ALPHA;
        self.build_frame_ms = self.build_frame_ms * prev + build_frame_ms * Self::ALPHA;
        self.scene_ms = self.scene_ms * prev + scene_ms * Self::ALPHA;
        self.present_ms = self.present_ms * prev + present_ms * Self::ALPHA;
        self.node_count = node_count;
    }
}

impl EventResponse {
    /// Response for an event the source did not consume.
    pub const IGNORED: Self = Self {
        handled: false,
        request_redraw: false,
        consume_key_text: false,
        text_input: None,
        clipboard: None,
    };

    /// Consume an event and request a new frame.
    pub const fn handled() -> Self {
        Self {
            handled: true,
            request_redraw: true,
            consume_key_text: false,
            text_input: None,
            clipboard: None,
        }
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn frame_budget_blends_stages_and_keeps_raw_counts() {
        let mut stats = FrameStats::default();
        stats.apply_budget(FrameBudgetSample {
            prepare_ms: 4.0,
            upload_ms: 2.0,
            encode_ms: 1.0,
            submit_ms: 3.0,
            submissions: 1,
            command_buffers: 1,
            wait_ms: 0.0,
            gpu_ms: Some(8.0),
            upload_bytes: 4096,
            instances: 700,
            draw_calls: 90,
        });
        assert!((stats.prepare_ms - 0.4).abs() < 1e-9);
        assert!((stats.upload_ms - 0.2).abs() < 1e-9);
        assert!((stats.encode_ms - 0.1).abs() < 1e-9);
        assert!((stats.submit_ms - 0.3).abs() < 1e-9);
        // The first GPU sample bootstraps the average at full weight.
        assert_eq!(stats.gpu_ms, Some(8.0));
        assert_eq!(stats.upload_bytes, 4096);
        assert_eq!(stats.submissions, 1);
        assert_eq!(stats.command_buffers, 1);
        assert_eq!(stats.instances, 700);
        assert_eq!(stats.draw_calls, 90);

        stats.apply_budget(FrameBudgetSample {
            prepare_ms: 0.0,
            upload_ms: 0.0,
            encode_ms: 0.0,
            submit_ms: 0.0,
            submissions: 2,
            command_buffers: 3,
            wait_ms: 0.0,
            gpu_ms: None,
            upload_bytes: 2048,
            instances: 10,
            draw_calls: 2,
        });
        assert!((stats.prepare_ms - 0.36).abs() < 1e-9);
        // A backend without timestamps keeps the previous GPU average.
        assert_eq!(stats.gpu_ms, Some(8.0));
        assert_eq!(stats.upload_bytes, 2048);
        assert_eq!(stats.command_buffers, 3);
    }
}
