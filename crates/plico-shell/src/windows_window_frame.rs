//! Native Win32 frame policy, following MyGO's hidden-titlebar approach.
//!
//! Winit retains the standard sizing/system-menu styles for an undecorated
//! top-level window and removes its caption through `WM_NCCALCSIZE`. Plico
//! keeps that native path, asks DWM for its shadow and Windows 11 contour,
//! and draws only the titlebar contents through the renderer.

use winit::platform::windows::{CornerPreference, WindowAttributesWindows, WindowExtWindows};
use winit::window::Window;

pub(crate) fn attributes(
    decorations: bool,
    has_shadow: bool,
    corner_radius: u32,
) -> WindowAttributesWindows {
    WindowAttributesWindows::default()
        .with_undecorated_shadow(!decorations && has_shadow)
        .with_corner_preference(corner_preference(decorations, corner_radius))
}

pub(crate) fn apply(window: &dyn Window, decorations: bool, has_shadow: bool, corner_radius: u32) {
    window.set_undecorated_shadow(!decorations && has_shadow);
    window.set_corner_preference(corner_preference(decorations, corner_radius));
}

fn corner_preference(decorations: bool, corner_radius: u32) -> CornerPreference {
    if decorations {
        CornerPreference::Default
    } else if corner_radius > 0 {
        CornerPreference::Round
    } else {
        CornerPreference::DoNotRound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_frames_use_system_corners_only_when_requested() {
        assert_eq!(corner_preference(true, 16), CornerPreference::Default);
        assert_eq!(corner_preference(false, 16), CornerPreference::Round);
        assert_eq!(corner_preference(false, 0), CornerPreference::DoNotRound);
    }
}
