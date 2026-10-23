use crate::Entity;
use crate::InvalidateChannels;
fn is_false(value: &bool) -> bool {
    !*value
}

/// Static labels and numeric business IDs require no per-frame String allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Static(&'static str),
    Number(u64),
    Auto(u32),
    /// Lexical declaration ID emitted by component/view macros.
    Generated(u64),
}
impl From<&'static str> for Key {
    fn from(key: &'static str) -> Self {
        Self::Static(key)
    }
}
impl From<u64> for Key {
    fn from(key: u64) -> Self {
        Self::Number(key)
    }
}
impl From<usize> for Key {
    fn from(key: usize) -> Self {
        Self::Number(key as u64)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Identity {
    pub parent: Entity,
    pub key: Key,
    pub scope: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Root,
    Overlay,
    Dialog,
    /// Noninteractive group carrying a host-timed tooltip description.
    TooltipAnchor,
    Column,
    Row,
    Text,
    TextInput,
    Svg,
    Shader,
    Slider,
    Button,
    ButtonBase,
    Switch,
    Checkbox,
}
impl Kind {
    pub const fn interactive(self) -> bool {
        matches!(
            self,
            Self::Slider
                | Self::Overlay
                | Self::Button
                | Self::ButtonBase
                | Self::Switch
                | Self::Checkbox
                | Self::TextInput
        )
    }

    pub(crate) fn leaf(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Shader
                | Self::Svg
                | Self::Button
                | Self::Switch
                | Self::Checkbox
                | Self::TextInput
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Color {
    Literal(u32),
    Foreground,
    Surface,
    Accent,
    OnAccent,
    Muted,
    Panel,
    Control,
    Selection,
    Border,
    ControlHover,
    ControlPressed,
    AccentHover,
    AccentPressed,
    Focus,
}
/// Shared timing presets for interaction feedback and property transitions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    None,
    Quick,
    #[default]
    Smooth,
    Emphasized,
}
impl Motion {
    pub const fn duration_ms(self) -> u16 {
        match self {
            Self::None => 0,
            Self::Quick => 140,
            Self::Smooth => 240,
            Self::Emphasized => 360,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}
/// Specialized semantics for composed popup controls, independent of a backend.
/// Portable generic font choice; Default uses the configured window family.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FontFamily {
    #[default]
    Default,
    SansSerif,
    Serif,
    Monospace,
}
impl FontFamily {
    pub fn is_default(&self) -> bool {
        *self == Self::Default
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WidgetRole {
    ComboBox,
    ListBox,
    Option,
    Menu,
    MenuButton,
    MenuItem,
}
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SliderSpec {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
}
impl SliderSpec {
    pub fn normalize(self, value: f64) -> Option<f64> {
        if !value.is_finite()
            || !self.min.is_finite()
            || !self.max.is_finite()
            || !(self.max - self.min).is_finite()
            || self.max <= self.min
            || !self.step.is_finite()
            || self.step <= 0.
        {
            return None;
        }
        let value = value.clamp(self.min, self.max);
        if value == self.max {
            return Some(self.max);
        }
        let steps = ((value - self.min) / self.step).round();
        Some(if steps.is_finite() {
            (self.min + steps * self.step).clamp(self.min, self.max)
        } else {
            value
        })
    }
}
#[derive(
    Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, InvalidateChannels,
)]
#[serde(default)]
pub struct Style {
    #[serde(default, skip_serializing_if = "crate::LayoutStyle::is_default")]
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub layout: crate::LayoutStyle,
    /// Starts a read-only text selection scope for descendant paragraphs.
    #[invalidates(HIT_TEST)]
    pub selectable: bool,
    /// Gaussian backdrop sigma in logical pixels, capped at 64.
    #[invalidates(PAINT)]
    pub backdrop_blur: f32,
    /// Background tint alpha, independent of child opacity.
    #[invalidates(PAINT)]
    pub background_alpha: f32,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub width: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub height: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub width_percent: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub height_percent: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub min_width: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub min_height: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub max_width: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub max_height: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub grow: f32,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub shrink: f32,
    /// Fill the containing block without participating in normal layout.
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub overlay: bool,
    /// Local absolute position in logical pixels; unlike overlay, this is not a portal.
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub absolute: Option<[f32; 2]>,
    /// Portable anchor identity on a trigger.
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub anchor_id: Option<u64>,
    #[invalidates(SEMANTICS)]
    pub role: Option<WidgetRole>,
    /// Position this overlay beside the corresponding trigger.
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub popup_anchor: Option<u64>,
    #[invalidates(SEMANTICS, HIT_TEST)]
    pub modal_open: bool,
    #[invalidates(HIT_TEST)]
    pub dismiss_on_escape: bool,
    #[invalidates(HIT_TEST)]
    pub dismiss_on_backdrop: bool,
    /// Host wake deadline for removing an exiting modal (in injected-clock ms).
    pub presence_until_ms: Option<u64>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub wrap: bool,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub clip: bool,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub scroll_y: bool,
    /// Smooth wheel/track paging; direct thumb dragging remains immediate.
    #[invalidates(HIT_TEST)]
    pub smooth_scroll: bool,
    /// Follow growing content while the host scroll position remains at its end.
    #[invalidates(HIT_TEST)]
    pub follow_end: bool,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub align: Option<Align>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub justify: Option<Justify>,
    #[invalidates(MEASURE, LAYOUT, PAINT, SEMANTICS)]
    pub font_weight: u16,
    #[serde(default, skip_serializing_if = "FontFamily::is_default")]
    #[invalidates(MEASURE, LAYOUT, PAINT, SEMANTICS)]
    pub font_family: FontFamily,
    #[invalidates(PAINT)]
    pub opacity: f32,
    /// Paint-only translation in logical pixels, applied after layout.
    #[invalidates(TRANSFORM, HIT_TEST, PAINT, SEMANTICS)]
    pub translate: [f32; 2],
    /// Paint-only scale around the center of the border box.
    #[invalidates(TRANSFORM, HIT_TEST, PAINT, SEMANTICS)]
    pub scale: [f32; 2],
    #[invalidates(PAINT)]
    pub radius: f32,
    /// Tween explicit dimensions, spacing and paint properties; zero snaps.
    #[invalidates(PAINT)]
    pub transition_ms: u16,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS)]
    pub gap: f32,
    #[invalidates(padding as horizontal_padding, LAYOUT, HIT_TEST, SEMANTICS)]
    #[invalidates(padding as vertical_padding, LAYOUT, HIT_TEST, SEMANTICS)]
    pub padding: f32,
    #[invalidates(padding_x as horizontal_padding, LAYOUT, HIT_TEST, SEMANTICS)]
    pub padding_x: Option<f32>,
    #[invalidates(padding_y as vertical_padding, LAYOUT, HIT_TEST, SEMANTICS)]
    pub padding_y: Option<f32>,
    #[invalidates(LAYOUT, HIT_TEST, SEMANTICS, PAINT)]
    pub border_width: f32,
    #[invalidates(PAINT)]
    pub border_color: Color,
    #[invalidates(PAINT)]
    pub hover_background: Option<Color>,
    #[invalidates(PAINT)]
    pub pressed_background: Option<Color>,
    #[invalidates(MEASURE, LAYOUT, PAINT, SEMANTICS)]
    pub font_size: u16,
    /// Keep this paragraph on one line and paint an ellipsis at its width limit.
    #[serde(default, skip_serializing_if = "is_false")]
    #[invalidates(MEASURE, LAYOUT, PAINT, SEMANTICS)]
    pub text_ellipsis: bool,
    #[invalidates(PAINT)]
    pub foreground: Color,
    #[invalidates(PAINT)]
    pub background: Option<Color>,
    #[invalidates(PAINT, SEMANTICS, HIT_TEST)]
    pub disabled: bool,
    #[invalidates(PAINT, SEMANTICS)]
    pub checked: bool,
    #[invalidates(PAINT, SEMANTICS)]
    pub input_revision: u64,
    #[invalidates(HIT_TEST)]
    pub input_navigation: bool,
    #[invalidates(PAINT)]
    pub shader_revision: u64,
    #[invalidates(PAINT, SEMANTICS)]
    pub numeric_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[invalidates(PAINT, SEMANTICS)]
    pub slider: Option<SliderSpec>,
    #[invalidates(SEMANTICS)]
    pub selected: Option<bool>,
    #[invalidates(SEMANTICS)]
    pub expanded: Option<bool>,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            layout: Default::default(),
            selectable: false,
            backdrop_blur: 0.,
            background_alpha: 1.,
            width: None,
            height: None,
            width_percent: None,
            height_percent: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            grow: 0.,
            shrink: 1.,
            overlay: false,
            absolute: None,
            anchor_id: None,
            role: None,
            popup_anchor: None,
            modal_open: false,
            dismiss_on_escape: true,
            dismiss_on_backdrop: true,
            presence_until_ms: None,
            wrap: false,
            clip: false,
            scroll_y: false,
            smooth_scroll: false,
            follow_end: false,
            align: None,
            justify: None,
            font_weight: 400,
            font_family: FontFamily::Default,
            opacity: 1.,
            translate: [0., 0.],
            scale: [1., 1.],
            radius: 0.,
            transition_ms: 0,
            gap: 0.,
            padding: 0.,
            padding_x: None,
            padding_y: None,
            border_width: 0.,
            border_color: Color::Border,
            hover_background: None,
            pressed_background: None,
            font_size: 16,
            text_ellipsis: false,
            foreground: Color::Foreground,
            background: None,
            disabled: false,
            checked: false,
            input_revision: 0,
            input_navigation: false,
            shader_revision: 0,
            numeric_revision: 0,
            slider: None,
            selected: None,
            expanded: None,
        }
    }
}
impl Style {
    pub fn horizontal_padding(self) -> f32 {
        self.padding_x.unwrap_or(self.padding)
    }
    pub fn vertical_padding(self) -> f32 {
        self.padding_y.unwrap_or(self.padding)
    }
    pub fn valid(self) -> bool {
        self.layout.valid()
            && self.width.is_none_or(|n| n.is_finite() && n >= 0.)
            && self.height.is_none_or(|n| n.is_finite() && n >= 0.)
            && [
                self.width_percent,
                self.height_percent,
                self.min_width,
                self.min_height,
                self.max_width,
                self.max_height,
                self.padding_x,
                self.padding_y,
            ]
            .into_iter()
            .all(|v| v.is_none_or(|v| v.is_finite() && v >= 0.))
            && self
                .absolute
                .is_none_or(|p| p.iter().all(|v| v.is_finite()))
            && self.grow.is_finite()
            && self.grow >= 0.
            && self.shrink.is_finite()
            && self.shrink >= 0.
            && (100..=900).contains(&self.font_weight)
            && self.backdrop_blur.is_finite()
            && (0. ..=64.).contains(&self.backdrop_blur)
            && self.background_alpha.is_finite()
            && (0. ..=1.).contains(&self.background_alpha)
            && self.opacity.is_finite()
            && (0. ..=1.).contains(&self.opacity)
            && self.translate.iter().all(|v| v.is_finite())
            && self.scale.iter().all(|v| v.is_finite() && *v >= 0.)
            && self.radius.is_finite()
            && self.radius >= 0.
            && self.gap.is_finite()
            && self.gap >= 0.
            && self.padding.is_finite()
            && self.padding >= 0.
            && self.border_width.is_finite()
            && self.border_width >= 0.
            && self.font_size > 0
    }
    pub(crate) fn themed(self, kind: Kind) -> bool {
        (self.border_width > 0. && !matches!(self.border_color, Color::Literal(_)))
            || kind.leaf() && !matches!(self.foreground, Color::Literal(_))
            || self
                .background
                .is_some_and(|c| !matches!(c, Color::Literal(_)))
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub visited: usize,
    pub created: usize,
    pub reused: usize,
    pub removed: usize,
    pub text_writes: usize,
    pub measure_calls: usize,
    pub layout_runs: usize,
    pub paint_updates: usize,
    pub order_changes: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, snafu::Snafu)]
pub enum FrameError {
    #[snafu(display("duplicate UI key {key:?} under {parent:?}"))]
    DuplicateKey { parent: Entity, key: Key },
    #[snafu(display("UI style has an invalid size or font"))]
    InvalidStyle,
    #[snafu(display("interactive controls cannot be nested inside a composite button"))]
    NestedControl,
    #[snafu(display("viewport width must be finite and positive"))]
    InvalidViewport,
    #[snafu(display("UI frame counter exhausted"))]
    EpochExhausted,
    #[snafu(display("declarations did not settle after eight passes"))]
    DidNotSettle,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct MeasureKey {
    pub text_revision: u64,
    pub font_size: u16,
    pub available: f32,
    pub width: Option<f32>,
    pub padding: f32,
    pub padding_y: f32,
    pub height: Option<f32>,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct LayoutKey {
    pub flow_count: usize,
    pub width: f32,
    pub height: Option<f32>,
    pub gap: f32,
    pub padding: f32,
    pub padding_y: f32,
}
/// A styled UTF-8 range within one shaped paragraph. Later spans override earlier ones.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextSpan {
    pub range: std::ops::Range<usize>,
    pub weight: u16,
    pub italic: bool,
    pub color: Option<Color>,
}
impl TextSpan {
    pub fn valid_for(&self, text: &str) -> bool {
        self.range.start <= self.range.end
            && self.range.end <= text.len()
            && text.is_char_boundary(self.range.start)
            && text.is_char_boundary(self.range.end)
            && (1..=1000).contains(&self.weight)
    }
}
pub(crate) struct Node {
    pub identity: Identity,
    pub kind: Kind,
    pub style: Style,
    /// Cached validation result for the retained style, including rejected frames.
    pub style_valid: bool,
    pub text: String,
    pub text_revision: u64,
    pub spans: Vec<TextSpan>,
    pub grid: Option<crate::GridTemplate>,
    pub shader: Option<crate::ShaderParams>,
    pub numeric: Option<crate::NumericRange>,
    pub children: Vec<Entity>,
    pub next_children: Vec<Entity>,
    pub touched: u64,
    pub auto: u32,
    pub scroll_observed: std::cell::Cell<bool>,
    pub order_dirty: bool,
    /// Invalidation accumulated where declarations mutate the node, drained by
    /// the commit pass. Replaces a retained style snapshot per node.
    pub pending_changes: Changes,
    pub paint_size: Option<Size>,
    pub measure_key: Option<MeasureKey>,
    pub layout_key: Option<LayoutKey>,
    pub size: Size,
}
impl Node {
    pub fn new(identity: Identity, kind: Kind) -> Self {
        Self {
            identity,
            kind,
            style: Style::default(),
            style_valid: true,
            text: String::new(),
            text_revision: 0,
            spans: Vec::new(),
            grid: None,
            shader: None,
            numeric: None,
            children: Vec::new(),
            next_children: Vec::new(),
            touched: 0,
            auto: 0,
            scroll_observed: std::cell::Cell::new(false),
            order_dirty: true,
            // A fresh node paints for the first time: every channel is stale.
            pending_changes: Changes::LAYOUT
                | Changes::PAINT
                | Changes::SEMANTICS
                | Changes::HIT_TEST
                | Changes::MEASURE,
            paint_size: None,
            measure_key: None,
            layout_key: None,
            size: Size::default(),
        }
    }
}
pub struct NodeRef<'a> {
    pub entity: Entity,
    pub kind: Kind,
    pub text: &'a str,
    pub spans: &'a [TextSpan],
    pub grid: Option<&'a crate::GridTemplate>,
    pub shader: Option<&'a crate::ShaderParams>,
    pub numeric: Option<crate::NumericRange>,
    pub style: Style,
    pub size: Size,
    pub children: &'a [Entity],
}

bitflags::bitflags! {
    /// Per-declaration requests and invalidations. VIEW requests a settling pass;
    /// the other bits describe diff results, not application-maintained flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
    pub struct Changes: u16 {
        const VIEW = 1 << 0;
        const LAYOUT = 1 << 1;
        const PAINT = 1 << 2;
        const SEMANTICS = 1 << 3;
        /// Schedule a future frame; never causes a same-frame settling pass.
        const ANIMATE = 1 << 4;
        const STRUCTURE = 1 << 5;
        /// Text shaping/measurement inputs changed, distinct from other layout.
        const MEASURE = 1 << 6;
        /// Presentation geometry changed without changing layout geometry.
        const TRANSFORM = 1 << 7;
        /// Input eligibility/routing or presented hit geometry changed.
        const HIT_TEST = 1 << 8;
    }
}

/// Path-addressed input events no declaration consumed in a settled pass.
#[derive(Debug, Default)]
pub struct UnmatchedPaths {
    pub clicks: Vec<Vec<crate::PathKey>>,
    pub values: Vec<(Vec<crate::PathKey>, f64)>,
}
/// Final per-node invalidation after one declaration, before host reconciliation.
#[derive(Debug, Clone, Copy)]
pub struct NodeChange {
    pub entity: crate::Entity,
    pub changes: Changes,
}

/// Everything an incremental backend needs from one settled frame.
///
/// The engine aggregates per-pass journals into this snapshot; taking it
/// consumes it, so a backend sees each change exactly once. New nodes always
/// appear in `changes` with every channel set — a backend never needs to
/// diff mounts against the arena. `removed` accumulates across the passes of
/// a settle loop; `reordered` children are reported through `STRUCTURE` on
/// the parent.
#[derive(Debug, Clone, Default)]
pub struct FrameDelta {
    /// Live nodes whose kind, style, text, order or children changed, with
    /// the union of change channels across settle passes.
    pub changes: Vec<NodeChange>,
    /// Entities unmounted since the last take.
    pub removed: Vec<crate::Entity>,
    /// The frame-wide change union; `VIEW` is cleared (the settle loop ended).
    pub frame: Changes,
}
