//! Renderer-independent keyboard events and application shortcuts.
bitflags::bitflags! {
    /// Keyboard modifiers active for an input event.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct Modifiers: u8 {
        /// Shift is held.
        const SHIFT = 1 << 0;
        /// Control is held.
        const CONTROL = 1 << 1;
        /// Alt/Option is held.
        const ALT = 1 << 2;
        /// Meta/Command/Windows is held.
        const META = 1 << 3;
    }
}

impl Modifiers {
    /// Whether Shift is held.
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }

    /// Whether Control is held.
    pub const fn control(self) -> bool {
        self.contains(Self::CONTROL)
    }

    /// Whether Alt/Option is held.
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }

    /// Whether Meta/Command/Windows is held.
    pub const fn meta(self) -> bool {
        self.contains(Self::META)
    }

    /// Platform primary shortcut modifier (Ctrl or Meta), excluding AltGr.
    pub const fn primary_shortcut(self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.meta() && !self.control() && !self.alt()
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.control() && !self.meta() && !self.alt()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Physical key transition phase.
pub enum KeyPhase {
    /// Key was pressed or auto-repeated.
    Down,
    /// Key was released.
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
/// Physical location of a key with duplicated legends.
pub enum KeyLocation {
    #[default]
    /// Main section or a key without a meaningful side.
    Standard,
    /// Left-hand instance of a modifier key.
    Left,
    /// Right-hand instance of a modifier key.
    Right,
    /// Numeric keypad.
    Numpad,
}

impl KeyLocation {
    /// Return the DOM-compatible numeric `location` value.
    pub const fn dom_code(self) -> u8 {
        match self {
            Self::Standard => 0,
            Self::Left => 1,
            Self::Right => 2,
            Self::Numpad => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Keyboard event retaining both logical and physical-key information.
pub struct KeyEvent {
    /// Press/release phase.
    pub phase: KeyPhase,
    /// Logical key after applying active modifiers.
    pub key: String,
    /// Logical key with shortcut modifiers removed.
    pub key_without_modifiers: String,
    /// Physical-key code suitable for shortcut identity.
    pub code: String,
    /// Text the key would commit under normal text-input handling.
    pub text: Option<String>,
    /// Text produced when all modifiers are included.
    pub text_with_all_modifiers: Option<String>,
    /// Physical section containing the key.
    pub location: KeyLocation,
    /// Active keyboard modifiers.
    pub modifiers: Modifiers,
    /// Whether this is an automatic repeat rather than the initial press.
    pub repeat: bool,
    /// Whether the platform synthesized this transition during focus recovery.
    pub synthetic: bool,
}

/// Editing shortcuts whose physical bindings follow platform conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StandardShortcut {
    /// Copy the current selection.
    Copy,
    /// Cut the current selection.
    Cut,
    /// Paste clipboard contents.
    Paste,
    /// Select all editable/selectable content.
    SelectAll,
}

impl KeyEvent {
    /// Match a platform-standard editing shortcut. Physical combinations such
    /// as terminal Control+C should continue to inspect `modifiers` directly.
    pub fn matches_standard_shortcut(&self, shortcut: StandardShortcut) -> bool {
        let key = match shortcut {
            StandardShortcut::Copy => "c",
            StandardShortcut::Cut => "x",
            StandardShortcut::Paste => "v",
            StandardShortcut::SelectAll => "a",
        };
        self.phase == KeyPhase::Down
            && self.modifiers.primary_shortcut()
            && !self.modifiers.shift()
            && (self.key_without_modifiers.eq_ignore_ascii_case(key)
                || self.key.eq_ignore_ascii_case(key))
    }
}

/// One logical-key or physical-code binding. Extra modifiers must match exactly.
/// Repeats are ignored unless explicitly enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    key: &'static str,
    modifiers: Modifiers,
    physical: bool,
    repeat: bool,
}
impl Shortcut {
    /// Bind a logical key (for example `"k"` or `"F5"`).
    pub const fn new(key: &'static str, modifiers: Modifiers) -> Self {
        Self {
            key,
            modifiers,
            physical: false,
            repeat: false,
        }
    }
    /// Bind Command on macOS and Control elsewhere.
    pub const fn primary(key: &'static str) -> Self {
        Self::new(
            key,
            if cfg!(target_os = "macos") {
                Modifiers::META
            } else {
                Modifiers::CONTROL
            },
        )
    }
    /// Match a physical keyboard code (for example `"KeyK"`).
    pub const fn physical(code: &'static str, modifiers: Modifiers) -> Self {
        Self {
            physical: true,
            ..Self::new(code, modifiers)
        }
    }
    /// Also require Shift.
    pub const fn shift(mut self) -> Self {
        self.modifiers = self.modifiers.union(Modifiers::SHIFT);
        self
    }
    /// Allow OS key-repeat events, for actions such as navigation.
    pub const fn repeating(mut self) -> Self {
        self.repeat = true;
        self
    }
    /// Match initial presses; never match synthetic focus-recovery events.
    pub fn matches(self, event: &KeyEvent) -> bool {
        let key = if self.physical {
            &event.code
        } else if !event.key_without_modifiers.is_empty()
            && event.key_without_modifiers != "Unidentified"
        {
            &event.key_without_modifiers
        } else {
            &event.key
        };
        event.phase == KeyPhase::Down
            && !event.synthetic
            && (!event.repeat || self.repeat)
            && event.modifiers == self.modifiers
            && self.key.eq_ignore_ascii_case(key)
    }
    fn conflicts(self, other: Self) -> bool {
        self.physical == other.physical
            && self.modifiers == other.modifiers
            && self.key.eq_ignore_ascii_case(other.key)
    }
}
/// A binding already exists in the same scope. No registration is overwritten.
#[derive(Debug, snafu::Snafu)]
#[snafu(display("shortcut {key} is already registered in scope {scope}"))]
pub struct ShortcutConflict {
    scope: String,
    key: &'static str,
}

/// Value-only action bindings. Construct once in application state (skip serialization).
/// The caller supplies active scopes from innermost to outermost; the first match wins.
/// This is window-local fallback routing, not OS-global hotkey registration.
pub struct ShortcutMap<A> {
    entries: Vec<(String, Shortcut, A)>,
}
impl<A> Default for ShortcutMap<A> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}
impl<A> ShortcutMap<A> {
    /// Register a binding. The same chord may be intentionally shadowed in another scope.
    pub fn register(
        &mut self,
        scope: impl Into<String>,
        shortcut: Shortcut,
        action: A,
    ) -> Result<(), ShortcutConflict> {
        let scope = scope.into();
        if self
            .entries
            .iter()
            .any(|(s, key, _)| *s == scope && key.conflicts(shortcut))
        {
            return Err(ShortcutConflict {
                scope,
                key: shortcut.key,
            });
        }
        self.entries.push((scope, shortcut, action));
        Ok(())
    }
    /// Resolve one action in the explicit active-scope order, without invoking callbacks.
    pub fn resolve(&self, event: &KeyEvent, scopes: &[&str]) -> Option<&A> {
        scopes.iter().find_map(|scope| {
            self.entries
                .iter()
                .find_map(|(s, key, action)| (s == scope && key.matches(event)).then_some(action))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event() -> KeyEvent {
        KeyEvent {
            phase: KeyPhase::Down,
            key: "K".into(),
            key_without_modifiers: "k".into(),
            code: "KeyK".into(),
            text: None,
            text_with_all_modifiers: None,
            location: KeyLocation::Standard,
            modifiers: Shortcut::primary("k").modifiers,
            repeat: false,
            synthetic: false,
        }
    }
    #[test]
    fn exact_modifiers_physical_keys_repeat_and_recovery() {
        let mut key = event();
        let binding = Shortcut::primary("k");
        assert!(binding.matches(&key));
        key.modifiers |= Modifiers::SHIFT;
        assert!(!binding.matches(&key));
        assert!(binding.shift().matches(&key));
        key.repeat = true;
        assert!(!binding.shift().matches(&key));
        assert!(binding.shift().repeating().matches(&key));
        key.synthetic = true;
        assert!(!binding.shift().repeating().matches(&key));
        key = event();
        key.key = "different layout".into();
        key.key_without_modifiers = "other".into();
        assert!(!binding.matches(&key));
        assert!(Shortcut::physical("KeyK", key.modifiers).matches(&key));
        key.phase = KeyPhase::Up;
        assert!(!Shortcut::physical("KeyK", key.modifiers).matches(&key));
    }
    #[test]
    fn scopes_shadow_without_overwriting_and_conflicts_are_reported() {
        let mut map = ShortcutMap::default();
        map.register("window", Shortcut::primary("k"), 1).unwrap();
        map.register("editor", Shortcut::primary("k"), 2).unwrap();
        assert!(
            map.register("window", Shortcut::primary("K").repeating(), 3)
                .is_err()
        );
        assert_eq!(map.resolve(&event(), &["editor", "window"]), Some(&2));
        assert_eq!(map.resolve(&event(), &["inactive", "window"]), Some(&1));
        assert_eq!(map.resolve(&event(), &["inactive"]), None);
    }
}
