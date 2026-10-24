//! Backend-local object classes, independent of core interaction capabilities.
use plico_core::Kind;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeClass {
    Container,
    Label,
    LabeledButton,
    Button,
    Editor,
    Image,
}
pub(crate) const fn class(kind: Kind) -> Option<NativeClass> {
    use NativeClass as C;
    match kind {
        Kind::Root
        | Kind::Column
        | Kind::Row
        | Kind::Overlay
        | Kind::Dialog
        | Kind::TooltipAnchor => Some(C::Container),
        Kind::Text => Some(C::Label),
        Kind::TextInput => Some(C::Editor),
        Kind::Button => Some(C::LabeledButton),
        Kind::ButtonBase => Some(C::Button),
        Kind::Svg => Some(C::Image),
        _ => None,
    }
}
pub(crate) const fn uses_flex(kind: Kind) -> bool {
    matches!(
        class(kind),
        Some(NativeClass::Container | NativeClass::LabeledButton | NativeClass::Button)
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_behavior_is_not_approximated_as_a_container() {
        for kind in [Kind::Switch, Kind::Checkbox, Kind::Slider, Kind::Shader] {
            assert_eq!(class(kind), None);
        }
        assert!(uses_flex(Kind::ButtonBase));
        assert!(!uses_flex(Kind::TextInput));
        assert!(!uses_flex(Kind::Text));
        assert!(!uses_flex(Kind::Svg));
    }
}
