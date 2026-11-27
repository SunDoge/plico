//! Shared visual recipes for painted controls.
//!
//! Widgets choose a semantic role; these functions own the MyGO-derived
//! mixing rules. A new control composes the recipes instead of copying
//! per-widget color literals, so theme changes stay in one place.
use plico::Theme;

use crate::{is_dark, mix_rgb};

/// Quiet but legible rim of unchecked boxes. CUI treats checkbox edges as the
/// control's primary silhouette, so it is stronger than a button hairline.
pub(crate) fn quiet_border(theme: Theme) -> u32 {
    mix_rgb(theme.border, theme.foreground, 0.35)
}
/// A switch track: `Border.Mix(Text, 0.15)` when off, filling to accent.
pub(crate) fn switch_track(theme: Theme, progress: f32) -> u32 {
    mix_rgb(
        mix_rgb(theme.border, theme.foreground, 0.15),
        theme.accent,
        progress,
    )
}
/// A checkbox face: the panel surface, filling to accent when checked.
pub(crate) fn checkbox_face(theme: Theme, progress: f32) -> u32 {
    mix_rgb(theme.panel, theme.accent, progress)
}
/// A checkbox rim: merging into the accent face, accent under the pointer
/// while unchecked, the quiet border otherwise.
pub(crate) fn checkbox_rim(theme: Theme, progress: f32, hovered: bool) -> u32 {
    if progress > 0. {
        mix_rgb(quiet_border(theme), theme.accent, progress)
    } else if hovered {
        theme.accent
    } else {
        quiet_border(theme)
    }
}
/// A physical thumb face: white in light, softened in the dark.
pub(crate) fn thumb_face(theme: Theme) -> u32 {
    if is_dark(theme) { 0xe0e1e1 } else { 0xffffff }
}
/// The rim of a held slider knob, at MyGO's 22% light / 70% dark alpha.
pub(crate) fn knob_rim_alpha(theme: Theme) -> f32 {
    if is_dark(theme) { 0.7 } else { 0.22 }
}
/// Fields are identified by their edge, so their resting border is stronger
/// than a button hairline and firms up under the pointer. Focus spends the
/// accent, following CUI's control-edge hierarchy on top of MyGO's states.
pub(crate) fn input_border(theme: Theme, focused: bool, hovered: bool) -> u32 {
    // CUI's strong neutral field edge sits near #aaa in light mode and #838383
    // in dark mode. Derive it from the active palette so custom themes follow.
    let strength = if theme.is_dark() { 0.37 } else { 0.28 };
    let edge = mix_rgb(theme.border, theme.foreground, strength);
    if focused {
        theme.accent
    } else if hovered {
        mix_rgb(edge, theme.muted, 0.5)
    } else {
        edge
    }
}
