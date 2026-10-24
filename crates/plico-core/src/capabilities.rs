//! Static input capabilities, separate from node payload and layout data.
use crate::Kind;

bitflags::bitflags! {
    /// Static input capabilities. These describe supported behavior, not current
    /// state, layout, accessibility roles, or a backend's implementation status.
    /// Derived from Kind; no extra flags are stored on each retained node.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Capabilities: u16 {
        /// Has an activation action, such as pressing a button or its backdrop.
        const ACTIVATE = 1 << 0;
        /// Can receive keyboard focus, including programmatic modal focus.
        const FOCUS = 1 << 1;
        /// Supports native text editing through a TextBuffer.
        const TEXT_EDIT = 1 << 2;
        /// Supports text selection; declarations may still disable/opt out of it.
        const TEXT_SELECT = 1 << 3;
        /// Supports editing a numeric value, not a generic click action.
        const VALUE_EDIT = 1 << 4;
        /// Participates in ordinary Tab navigation. Always implies FOCUS.
        /// A dialog can receive focus without becoming one of its own Tab stops.
        const TAB_STOP = 1 << 5;
    }
}
impl Kind {
    /// Compile-time capability mapping; widgets composed from existing nodes
    /// do not need a new kind, registration, or a dynamic dispatch interface.
    #[inline]
    pub const fn capabilities(self) -> Capabilities {
        use Capabilities as C;
        match self {
            Self::Button | Self::ButtonBase | Self::Switch | Self::Checkbox => {
                C::ACTIVATE.union(C::FOCUS).union(C::TAB_STOP)
            }
            Self::TextInput => C::TEXT_EDIT
                .union(C::TEXT_SELECT)
                .union(C::FOCUS)
                .union(C::TAB_STOP),
            Self::Slider => C::VALUE_EDIT.union(C::FOCUS).union(C::TAB_STOP),
            Self::Text => C::TEXT_SELECT,
            Self::Overlay => C::ACTIVATE,
            Self::Dialog => C::FOCUS,
            // New structural/media nodes are noninteractive until explicitly
            // given input behavior. Backends still choose their own support.
            _ => C::empty(),
        }
    }
}
