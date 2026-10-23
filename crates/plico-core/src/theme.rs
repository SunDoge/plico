use crate::Color;

/// Shared component measurements. A copy belongs to the theme, not to each widget.
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
            radius: 6,
            font_size: 14,
            label_size: 12,
            title_size: 26,
            control_height: 32,
            interaction_ms: 140,
        }
    }
}
impl Metrics {
    pub fn compact() -> Self {
        Self {
            spacing: 3,
            radius: 5,
            font_size: 13,
            control_height: 28,
            ..Self::default()
        }
    }
    pub const fn space(self, units: u16) -> u16 {
        self.spacing.saturating_mul(units)
    }
}

/// Semantic colors and measurements shared by default and application components.
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
    pub metrics: Metrics,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            foreground: 0x202024,
            surface: 0xf7f7f8,
            accent: 0x2266dd,
            on_accent: 0xffffff,
            muted: 0x71717a,
            panel: 0xffffff,
            control: 0xf4f4f5,
            selection: 0xdde9ff,
            border: 0xd9d9de,
            control_hover: 0xe9e9ec,
            control_pressed: 0xdddde1,
            accent_hover: 0x1d4ed8,
            accent_pressed: 0x1e40af,
            focus: 0x6094ef,
            metrics: Metrics::default(),
        }
    }
}
impl Theme {
    pub fn light() -> Self {
        Self {
            foreground: 0x202024,
            surface: 0xf7f7f8,
            accent: 0x285bd6,
            ..Self::default()
        }
    }
    pub fn dark() -> Self {
        Self {
            foreground: 0xf4f4f5,
            surface: 0x18181b,
            accent: 0x94b5ff,
            on_accent: 0x10213e,
            muted: 0xa1a1aa,
            panel: 0x202023,
            control: 0x27272a,
            selection: 0x263d65,
            border: 0x3f3f46,
            control_hover: 0x323236,
            control_pressed: 0x3c3c41,
            accent_hover: 0xb0c8ff,
            accent_pressed: 0x78a0ed,
            focus: 0x94b5ff,
            metrics: Metrics::default(),
        }
    }
    pub fn same_colors(mut self, other: Self) -> bool {
        self.metrics = other.metrics;
        self == other
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
        }
    }
}
