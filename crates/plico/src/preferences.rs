//! Desktop accessibility preferences, following MyGO ui/preferences.go.
//! Source: ee3d4bd9bd741f197de231b9fd9b002e9db55577, MIT; see third_party/.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Accessibility display preferences supplied by the host, not persisted app state.
pub struct DisplayPreferences {
    /// Prefer less non-essential motion.
    pub reduce_motion: bool,
    /// Replace translucent materials with opaque surfaces.
    pub reduce_transparency: bool,
    /// Strengthen semantic borders and secondary text contrast.
    pub high_contrast: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, Theme};
    #[test]
    fn contrast_recomputes_from_the_requested_theme_and_restores_exactly() {
        let mut engine = Engine::default();
        let mut theme = Theme::light();
        theme.muted = 0x808080;
        theme.border = 0xeeeeee;
        engine.set_theme(theme);
        let contrast = DisplayPreferences {
            high_contrast: true,
            ..Default::default()
        };
        assert!(engine.set_display_preferences(contrast));
        let adjusted = engine.theme();
        assert_ne!(adjusted.border, theme.border);
        assert_ne!(adjusted.muted, theme.muted);
        assert_eq!(adjusted.accent, theme.accent);
        assert_eq!(adjusted.foreground, theme.foreground);
        assert!(!engine.set_display_preferences(contrast));
        engine.set_theme(theme);
        assert_eq!(
            engine.theme(),
            adjusted,
            "re-declarations must not compound the color mix"
        );
        engine.set_display_preferences(Default::default());
        assert_eq!(engine.theme(), theme);
        engine.set_display_preferences(contrast);
        let dark = Theme::dark();
        engine.set_theme(dark);
        assert_eq!(engine.theme(), dark.follow_preferences(contrast));
        engine.set_display_preferences(Default::default());
        assert_eq!(engine.theme(), dark);
    }
}
