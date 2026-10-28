//! Backend-neutral native-window requests.

use serde::{Deserialize, Serialize};

/// Initial stacking level requested for a native window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowLevel {
    /// Keep the window below ordinary application windows where supported.
    AlwaysOnBottom,
    /// Use the platform's ordinary application-window level.
    #[default]
    Normal,
    /// Keep the window above ordinary application windows where supported.
    AlwaysOnTop,
}

/// Whether a native window participates in pointer hit testing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowInputMode {
    /// Deliver pointer input to the window normally.
    #[default]
    Interactive,
    /// Let pointer input pass through to windows underneath where supported.
    Passthrough,
}

/// Native compositor material used behind transparent window content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowBackground {
    /// Fully opaque window background.
    #[default]
    Opaque,
    /// Preserve alpha without applying a system backdrop effect.
    Transparent,
    /// Ask the platform compositor to blur content behind the window.
    Blurred,
    /// Windows 11 Mica material; unsupported platforms may fall back.
    Mica,
    /// Windows 11 Mica Alt material; unsupported platforms may fall back.
    MicaAlt,
}

/// Graphics consumer selected by the native window host.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderBackend {
    /// Default Vello GPU consumer.
    #[default]
    Vello,
    /// Experimental Skia Vulkan consumer; requires the shell's `skia` feature.
    Skia,
    /// Experimental native egui controls with Taffy geometry; requires the
    /// shell's `egui` feature and a native-widget frame source.
    Egui,
}

bitflags::bitflags! {
    /// Edges that may start a resize gesture on a custom, undecorated window.
    /// Native decorations and compositor shortcuts are controlled by the OS;
    /// use minimum/maximum sizes to constrain dimensions for those paths too.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct WindowResizeEdges: u8 {
        /// Left border.
        const LEFT = 1;
        /// Right border.
        const RIGHT = 2;
        /// Top border.
        const TOP = 4;
        /// Bottom border.
        const BOTTOM = 8;
    }
}
impl Default for WindowResizeEdges {
    fn default() -> Self {
        Self::all()
    }
}

/// Requested properties used when creating a native window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowOptions {
    /// Renderer used by the native window host.
    #[serde(default)]
    pub renderer: RenderBackend,
    /// Initial native window title.
    pub title: String,
    /// Requested logical client-area size.
    pub initial_inner_size: (u32, u32),
    /// Optional minimum logical client-area size.
    pub min_inner_size: Option<(u32, u32)>,
    /// Optional maximum logical client-area size. A conflicting minimum wins.
    #[serde(default)]
    pub max_inner_size: Option<(u32, u32)>,
    /// Allowed custom-decoration resize edges. Does not restrict native borders.
    #[serde(default)]
    pub resize_edges: WindowResizeEdges,
    /// Whether the user may resize the window.
    pub resizable: bool,
    /// Whether the OS draws its standard title bar and border.
    pub decorations: bool,
    /// Visual client-area corner radius in logical pixels. Requires compositor alpha.
    #[serde(default)]
    pub corner_radius: u32,
    /// Whether to place the initial window in the center of the primary monitor.
    #[serde(default)]
    pub centered: bool,
    /// Native compositor material visible through transparent app content.
    pub background: WindowBackground,
    /// Requested initial stacking level.
    pub window_level: WindowLevel,
    /// Requested initial pointer hit-test behavior.
    pub input_mode: WindowInputMode,
    /// Whether the first show requests keyboard focus. Platform dependent.
    #[serde(default = "default_activate_on_show")]
    pub activate_on_show: bool,
}

fn default_activate_on_show() -> bool {
    true
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            renderer: RenderBackend::Vello,
            title: "wabou".into(),
            initial_inner_size: (800, 600),
            min_inner_size: None,
            max_inner_size: None,
            resize_edges: WindowResizeEdges::all(),
            resizable: true,
            decorations: true,
            corner_radius: 0,
            centered: false,
            background: WindowBackground::Opaque,
            window_level: WindowLevel::Normal,
            input_mode: WindowInputMode::Interactive,
            activate_on_show: true,
        }
    }
}

impl WindowOptions {
    /// Construct options with platform-friendly defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request a transparent, undecorated, pointer-transparent floating window.
    ///
    /// Stacking and pointer passthrough depend on the window manager. In
    /// particular, a normal Wayland window is not a layer-shell surface and
    /// cannot guarantee overlay stacking or a screen-relative position.
    /// A frame source must also use a transparent base color to expose alpha.
    pub fn floating_overlay() -> Self {
        Self::new()
            .decorations(false)
            .background(WindowBackground::Transparent)
            .window_level(WindowLevel::AlwaysOnTop)
            .input_mode(WindowInputMode::Passthrough)
            .activate_on_show(false)
    }

    /// Select the graphics consumer without changing the application source.
    pub fn renderer(mut self, renderer: RenderBackend) -> Self {
        self.renderer = renderer;
        self
    }

    /// Set the initial native window title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the requested logical client-area size, clamped to at least 1×1.
    pub fn initial_inner_size(mut self, width: u32, height: u32) -> Self {
        self.initial_inner_size = (width.max(1), height.max(1));
        self
    }

    /// Set the minimum logical client-area size, clamped to at least 1×1.
    pub fn min_inner_size(mut self, width: u32, height: u32) -> Self {
        self.min_inner_size = Some((width.max(1), height.max(1)));
        self
    }

    /// Set the maximum logical client-area size, clamped to at least 1×1.
    /// Equal minimum and maximum widths/heights lock that dimension where
    /// supported by the window manager. A conflicting minimum takes precedence.
    pub fn max_inner_size(mut self, width: u32, height: u32) -> Self {
        self.max_inner_size = Some((width.max(1), height.max(1)));
        self
    }

    /// Select draggable edges for custom decorations. Native decorations and
    /// OS shortcuts are unaffected; `resizable(false)` disables all edges.
    pub fn resize_edges(mut self, edges: WindowResizeEdges) -> Self {
        self.resize_edges = edges;
        self
    }

    /// Normalize size constraints and clamp the initial size. Minimums take
    /// precedence over conflicting maximums, independently of builder order.
    pub fn normalized(mut self) -> Self {
        let minimum = self.min_inner_size.map(|(w, h)| (w.max(1), h.max(1)));
        self.min_inner_size = minimum;
        let floor = minimum.unwrap_or((1, 1));
        self.max_inner_size = self
            .max_inner_size
            .map(|(w, h)| (w.max(floor.0), h.max(floor.1)));
        let ceiling = self.max_inner_size.unwrap_or((u32::MAX, u32::MAX));
        self.initial_inner_size = (
            self.initial_inner_size.0.clamp(floor.0, ceiling.0),
            self.initial_inner_size.1.clamp(floor.1, ceiling.1),
        );
        self
    }

    /// Custom resize edges after applying `resizable` and locked dimensions.
    pub fn effective_resize_edges(&self) -> WindowResizeEdges {
        if !self.resizable {
            return WindowResizeEdges::empty();
        }
        let mut edges = self.resize_edges;
        if let (Some(min), Some(max)) = (self.min_inner_size, self.max_inner_size) {
            if min.0 >= max.0 {
                edges.remove(WindowResizeEdges::LEFT | WindowResizeEdges::RIGHT);
            }
            if min.1 >= max.1 {
                edges.remove(WindowResizeEdges::TOP | WindowResizeEdges::BOTTOM);
            }
        }
        edges
    }

    /// Control whether the user may resize the window.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Clip client content to rounded corners on an alpha-capable native surface.
    /// Native decorations and the OS input region remain separate.
    pub fn corner_radius(mut self, radius: u32) -> Self {
        self.corner_radius = radius;
        self
    }

    /// Enable or disable native window-manager decorations.
    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Place the initial window in the center of the primary monitor when supported.
    pub fn centered(mut self, centered: bool) -> Self {
        self.centered = centered;
        self
    }

    /// Select the native compositor material behind transparent app content.
    pub fn background(mut self, background: WindowBackground) -> Self {
        self.background = background;
        self
    }

    /// Set the initial native stacking level.
    pub fn window_level(mut self, window_level: WindowLevel) -> Self {
        self.window_level = window_level;
        self
    }

    /// Request whether the first show activates this window, where supported.
    pub fn activate_on_show(mut self, activate: bool) -> Self {
        self.activate_on_show = activate;
        self
    }

    /// Set the initial pointer hit-test behavior.
    pub fn input_mode(mut self, input_mode: WindowInputMode) -> Self {
        self.input_mode = input_mode;
        self
    }
}

/// Command addressed to a Wabou-managed native window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowCommand {
    /// Request that the window close.
    Close,
    /// Minimize the window.
    Minimize,
    /// Set or clear the maximized state.
    SetMaximized(bool),
    /// Replace the native window title.
    SetTitle(String),
    /// Begin an OS-managed window drag from the current pointer gesture.
    StartDragging,
    /// Restore a hidden or surface-released logical window.
    Show,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_clamp_invalid_initial_dimensions_at_the_contract_boundary() {
        let options = WindowOptions::new()
            .initial_inner_size(0, 0)
            .min_inner_size(0, 0);

        assert_eq!(options.initial_inner_size, (1, 1));
        assert_eq!(options.min_inner_size, Some((1, 1)));
    }

    #[test]
    fn window_options_have_a_stable_serialized_contract() {
        let options = WindowOptions::new()
            .title("Contract")
            .background(WindowBackground::Transparent)
            .input_mode(WindowInputMode::Passthrough)
            .window_level(WindowLevel::AlwaysOnTop);
        let encoded = serde_json::to_string(&options).expect("serialize window options");
        let decoded: WindowOptions =
            serde_json::from_str(&encoded).expect("deserialize window options");

        assert_eq!(decoded, options);
    }
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    #[test]
    fn old_window_options_keep_vello_and_skia_roundtrips() {
        let mut value = serde_json::to_value(WindowOptions::new()).unwrap();
        value.as_object_mut().unwrap().remove("renderer");
        value.as_object_mut().unwrap().remove("activateOnShow");
        let old: WindowOptions = serde_json::from_value(value).unwrap();
        assert_eq!(old.renderer, RenderBackend::Vello);
        assert!(old.activate_on_show);
        let options = WindowOptions::new().renderer(RenderBackend::Skia);
        let restored: WindowOptions =
            serde_json::from_str(&serde_json::to_string(&options).unwrap()).unwrap();
        assert_eq!(restored.renderer, RenderBackend::Skia);
    }
}

#[cfg(test)]
mod resize_tests {
    use super::*;

    #[test]
    fn old_options_default_to_unbounded_size_and_all_edges() {
        let mut json = serde_json::to_value(WindowOptions::new()).unwrap();
        let object = json.as_object_mut().unwrap();
        object.remove("maxInnerSize");
        object.remove("resizeEdges");
        let options: WindowOptions = serde_json::from_value(json).unwrap();
        assert_eq!(options.max_inner_size, None);
        assert_eq!(options.resize_edges, WindowResizeEdges::all());
        let options = options.resize_edges(WindowResizeEdges::BOTTOM | WindowResizeEdges::RIGHT);
        let restored: WindowOptions =
            serde_json::from_str(&serde_json::to_string(&options).unwrap()).unwrap();
        assert_eq!(restored, options);
    }

    #[test]
    fn size_bounds_lock_one_axis_and_clamp_the_initial_size() {
        let options = WindowOptions::new()
            .initial_inner_size(900, 100)
            .min_inner_size(480, 240)
            .max_inner_size(480, 900)
            .normalized();
        assert_eq!(options.initial_inner_size, (480, 240));
        assert_eq!(
            options.effective_resize_edges(),
            WindowResizeEdges::TOP | WindowResizeEdges::BOTTOM
        );
        let horizontal = WindowOptions::new()
            .min_inner_size(320, 400)
            .max_inner_size(1200, 400)
            .normalized();
        assert_eq!(
            horizontal.effective_resize_edges(),
            WindowResizeEdges::LEFT | WindowResizeEdges::RIGHT
        );
        assert!(options.resizable(false).effective_resize_edges().is_empty());
        let fixed = WindowOptions::new()
            .min_inner_size(480, 400)
            .max_inner_size(480, 400)
            .normalized();
        assert!(fixed.effective_resize_edges().is_empty());
    }

    #[test]
    fn minimum_wins_over_conflicting_maximum_in_either_builder_order() {
        let a = WindowOptions::new()
            .max_inner_size(200, 100)
            .min_inner_size(400, 300)
            .normalized();
        let b = WindowOptions::new()
            .min_inner_size(400, 300)
            .max_inner_size(200, 100)
            .normalized();
        assert_eq!(a, b);
        assert_eq!(a.max_inner_size, Some((400, 300)));
        assert_eq!(
            WindowOptions::new()
                .max_inner_size(0, 0)
                .normalized()
                .initial_inner_size,
            (1, 1)
        );
    }
}
