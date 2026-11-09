//! MyGO's drag-region double-click policy, kept independent from AppKit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TitlebarAction {
    Ignore,
    Minimize,
    Zoom,
}
pub(crate) fn titlebar_action(
    left_down: bool,
    same_window: bool,
    click_count: isize,
    preference: Option<&str>,
) -> Option<TitlebarAction> {
    if !left_down || !same_window || click_count != 2 {
        return None;
    }
    Some(match preference {
        Some("None") => TitlebarAction::Ignore,
        Some("Minimize") => TitlebarAction::Minimize,
        _ => TitlebarAction::Zoom,
    })
}
/// One native mouse event can be observed by more than one declaration pass.
/// Remember only its identity, so repeated commands cannot toggle zoom twice.
#[derive(Default)]
pub(crate) struct TitlebarEvent {
    last: std::cell::Cell<Option<(isize, u64)>>,
}
impl TitlebarEvent {
    pub fn consume_once(&self, number: isize, timestamp: f64) -> bool {
        let identity = (number, timestamp.to_bits());
        self.last.replace(Some(identity)) != Some(identity)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_drags_and_events_for_other_windows_never_trigger_titlebar_actions() {
        for count in [0, 1, 3, 4] {
            assert_eq!(titlebar_action(true, true, count, Some("Minimize")), None);
        }
        assert_eq!(titlebar_action(false, true, 2, Some("Minimize")), None);
        assert_eq!(titlebar_action(true, false, 2, Some("Minimize")), None);
    }
    #[test]
    fn native_double_click_can_explicitly_disable_the_action_or_minimize() {
        assert_eq!(
            titlebar_action(true, true, 2, Some("None")),
            Some(TitlebarAction::Ignore)
        );
        assert_eq!(
            titlebar_action(true, true, 2, Some("Minimize")),
            Some(TitlebarAction::Minimize)
        );
        for preference in [None, Some("Maximize"), Some("Zoom"), Some("unknown")] {
            assert_eq!(
                titlebar_action(true, true, 2, preference),
                Some(TitlebarAction::Zoom)
            );
        }
    }
    #[test]
    fn repeated_commands_for_one_event_cannot_toggle_window_state_twice() {
        let event = TitlebarEvent::default();
        assert!(event.consume_once(10, 1.));
        assert!(!event.consume_once(10, 1.));
        assert!(event.consume_once(11, 1.1));
        assert!(!event.consume_once(11, 1.1));
        assert!(
            event.consume_once(11, 2.),
            "timestamp distinguishes reused event numbers"
        );
    }
}
