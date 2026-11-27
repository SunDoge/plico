// Interaction colors follow MyGO ui/theme.go at
// 9b26576752f47b8334b67ed60bd4ce0f663cc6de. Default geometry and surface
// discipline adapt CUI's design system at
// 6e6b35326f655405822fcf863fac0d3bb35bb787. Both are MIT; see licenses/.
use crate::Color;

/// Shared component measurements. A copy belongs to the theme, not to each widget.
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Metrics {
    pub spacing: u16,
    pub radius: u16,
    pub font_size: u16,
    pub label_size: u16,
    pub title_size: u16,
    pub control_height: u16,
    pub interaction_ms: u16,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
            spacing: 4,
            radius: 10,
            font_size: 13,
            label_size: 12,
            title_size: 28,
            control_height: 36,
            interaction_ms: 140,
        }
    }
}
impl Metrics {
    /// Dense controls retain the 4-point grid and the same visual hierarchy.
    pub fn compact() -> Self {
        Self {
            radius: 8,
            font_size: 12,
            label_size: 11,
            title_size: 24,
            control_height: 30,
            ..Self::default()
        }
    }
    pub const fn space(self, units: u16) -> u16 {
        self.spacing.saturating_mul(units)
    }
    /// Radius for menus, popovers and nested panels.
    pub const fn panel_radius(self) -> u16 {
        self.radius.saturating_add(self.spacing)
    }
    /// Radius for cards, dialogs and sheets.
    pub const fn surface_radius(self) -> u16 {
        self.radius.saturating_add(self.spacing.saturating_mul(2))
    }
    /// A concentric inner radius after inset content by `padding` points.
    pub const fn inset_radius(self, outer: u16, padding: u16) -> u16 {
        outer.saturating_sub(padding)
    }
    /// Small, medium and large controls form the 30/36/44 default scale.
    pub const fn control_small(self) -> u16 {
        self.control_height.saturating_sub(6)
    }
    pub const fn control_medium(self) -> u16 {
        self.control_height
    }
    pub const fn control_large(self) -> u16 {
        self.control_height.saturating_add(8)
    }
    /// Vertical inset for buttons and fields at the active desktop density.
    pub(crate) fn control_padding_y(self) -> f32 {
        f32::from(self.spacing) * if self.control_height <= 30 { 1. } else { 1.5 }
    }
    pub(crate) fn apply_control_frame(self, style: &mut crate::Style) {
        style.min_height = Some(f32::from(self.control_height));
        style.radius = f32::from(self.radius);
        style.transition_ms = self.interaction_ms;
        style.padding = 0.;
        style.padding_y = Some(self.control_padding_y());
    }
    /// Even icon sizes stay aligned to physical-pixel grids at common scales.
    pub const fn icon_small(self) -> u16 {
        14
    }
    pub const fn icon_medium(self) -> u16 {
        16
    }
    pub const fn icon_large(self) -> u16 {
        18
    }
    /// Icon size paired with the active control density.
    pub const fn control_icon(self) -> u16 {
        if self.control_height <= 30 {
            self.icon_small()
        } else if self.control_height >= 44 {
            self.icon_large()
        } else {
            self.icon_medium()
        }
    }
}

/// Semantic colors and measurements shared by default and application components.
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Theme {
    pub foreground: u32,
    pub surface: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub muted: u32,
    pub panel: u32,
    pub control: u32,
    pub selection: u32,
    pub border: u32,
    pub control_hover: u32,
    pub control_pressed: u32,
    pub accent_hover: u32,
    pub accent_pressed: u32,
    pub focus: u32,
    /// Informational content; see [`Color::Info`].
    pub info: u32,
    /// Good or complete state; see [`Color::Success`].
    pub success: u32,
    /// Attention state; see [`Color::Warning`].
    pub warning: u32,
    /// Critical state; see [`Color::Danger`].
    pub danger: u32,
    pub metrics: Metrics,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            foreground: 0x18181b,
            surface: 0xf8f8fa,
            accent: 0x2563eb,
            on_accent: 0xffffff,
            muted: 0x686870,
            panel: 0xffffff,
            control: 0xf4f4f5,
            selection: 0xdce7fc,
            border: 0xe1e1e6,
            control_hover: 0xe9e9ec,
            control_pressed: 0xdddde1,
            accent_hover: 0x1d4ed8,
            accent_pressed: 0x1e40af,
            focus: 0x87a9f4,
            info: 0x1d4ed8,
            success: 0x16a34a,
            warning: 0xd97706,
            danger: 0xdc2626,
            metrics: Metrics::default(),
        }
    }
}
impl Theme {
    /// Follow native contrast preferences without accumulating adjustments.
    /// MyGO ui/preferences.go uses 45% foreground for borders and 40% for
    /// secondary text. Plico's focus token is already opaque.
    pub fn follow_preferences(mut self, preferences: crate::DisplayPreferences) -> Self {
        if preferences.high_contrast {
            let mix = |color: u32, percent: u32| -> u32 {
                [0, 8, 16].into_iter().fold(0, |rgb, shift| {
                    let from = (color >> shift) & 255;
                    let to = (self.foreground >> shift) & 255;
                    rgb | (((from * (100 - percent) + to * percent + 50) / 100) << shift)
                })
            };
            self.border = mix(self.border, 45);
            self.muted = mix(self.muted, 40);
        }
        self
    }
    pub fn light() -> Self {
        Self::default()
    }
    /// Whether the window surface reads as dark, using sRGB luminance weights.
    pub fn is_dark(self) -> bool {
        let [r, g, b] = [16, 8, 0].map(|shift| ((self.surface >> shift) & 255) as f32);
        0.2126 * r + 0.7152 * g + 0.0722 * b < 128.
    }
    pub fn dark() -> Self {
        Self {
            foreground: 0xf4f4f5,
            surface: 0x171719,
            accent: 0x3b82f6,
            on_accent: 0x0b1020,
            muted: 0xa1a1aa,
            panel: 0x202023,
            control: 0x28282c,
            selection: 0x264273,
            border: 0x39393f,
            control_hover: 0x323236,
            control_pressed: 0x3c3c41,
            accent_hover: 0x60a5fa,
            accent_pressed: 0x427fea,
            focus: 0x436da1,
            info: 0x93c5fd,
            success: 0x22c55e,
            warning: 0xf59e0b,
            danger: 0xef4444,
            metrics: Metrics::default(),
        }
    }
    pub fn same_colors(mut self, other: Self) -> bool {
        self.metrics = other.metrics;
        self == other
    }
    /// Blend `from` toward `to` by `amount` in `0..=1`, resolving both tokens
    /// against this theme (MyGO's `Color.Mix`). Useful for derived control
    /// borders without adding another theme token.
    pub fn blend(self, from: Color, to: Color, amount: f32) -> Color {
        let amount = amount.clamp(0., 1.);
        let (from, to) = (self.resolve(from), self.resolve(to));
        let mix = |shift: u32| {
            let from = ((from >> shift) & 255) as f32;
            let to = ((to >> shift) & 255) as f32;
            (from + (to - from) * amount).round().clamp(0., 255.) as u32
        };
        Color::Literal((mix(16) << 16) | (mix(8) << 8) | mix(0))
    }
    pub const fn resolve(self, color: Color) -> u32 {
        match color {
            Color::Literal(v) => v,
            Color::Foreground => self.foreground,
            Color::Surface => self.surface,
            Color::Accent => self.accent,
            Color::OnAccent => self.on_accent,
            Color::Muted => self.muted,
            Color::Panel => self.panel,
            Color::Control => self.control,
            Color::Selection => self.selection,
            Color::Border => self.border,
            Color::ControlHover => self.control_hover,
            Color::ControlPressed => self.control_pressed,
            Color::AccentHover => self.accent_hover,
            Color::AccentPressed => self.accent_pressed,
            Color::Focus => self.focus,
            Color::Info => self.info,
            Color::Success => self.success,
            Color::Warning => self.warning,
            Color::Danger => self.danger,
        }
    }
}
