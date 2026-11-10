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
            info: 0x1d4ed8,
            success: 0x15803d,
            warning: 0x92400e,
            danger: 0xb91c1c,
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
            info: 0x93c5fd,
            success: 0x86efac,
            warning: 0xfcd34d,
            danger: 0xfca5a5,
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
