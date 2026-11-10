//! Shared visual recipes for painted controls.
//!
//! Widgets choose a semantic role; these functions own the MyGO-derived
//! mixing rules. A new control composes the recipes instead of copying
//! per-widget color literals, so theme changes stay in one place.
use plico::Theme;

use crate::{is_dark, mix_rgb};

/// MyGO `Border.Mix(Text, 0.25)`: the quiet rim of unchecked boxes and
/// hovered text fields.
pub(crate) fn quiet_border(theme: Theme) -> u32 {
    mix_rgb(theme.border, theme.foreground, 0.25)
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
/// MyGO `inputBorder`: accent while focused, quiet border under the pointer,
/// the theme border otherwise.
pub(crate) fn input_border(theme: Theme, focused: bool, hovered: bool) -> u32 {
    if focused {
        theme.accent
    } else if hovered {
        quiet_border(theme)
    } else {
        theme.border
    }
}
