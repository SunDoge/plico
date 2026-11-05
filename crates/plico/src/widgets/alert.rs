//! A modal alert ported from MyGO's `ui/feedback.go` (`AlertDialog`).
use crate::WidgetRole;
use crate::prelude::*;

/// The value [`alert_dialog`] returns in the frame Escape closes the alert.
pub const ALERT_DISMISSED: i32 = -2;

/// A modal alert while `state` is open (MyGO `AlertDialog`): the title, the
/// message, and the `buttons`, the last of which is the default in the accent
/// color. It returns the index of the button clicked in the frame it closes
/// the alert, [`ALERT_DISMISSED`] in the frame Escape closes it, and -1
/// otherwise. A click outside does nothing.
///
/// The panel carries the alert-dialog role and is named by its title; the
/// message is visible text rather than a described-by relation. MyGO also puts
/// the keyboard focus on the default button so Enter answers the alert; Plico
/// has no general button focus request yet, so the first Tab reaches the
/// buttons.
///
/// ```text
/// match alert_dialog(cx, &mut asking, "Delete \u{201c}Notes\u{201d}?", "You can't undo this.", &["Cancel", "Delete"]) {
///     1 => delete(),
///     ALERT_DISMISSED => asking.open(),
///     _ => {}
/// }
/// ```
///
/// React to the index in the declaration pass that returns it. A settlement
/// pass can run again after the alert closes, sees the closed state and
/// returns -1, so a value stored for later observation is not reliable. If an
/// alert must be answered, reopen its state when [`ALERT_DISMISSED`] comes
/// back.
pub fn alert_dialog(
    cx: &mut UiContext,
    state: &mut ModalState,
    title: &str,
    message: &str,
    buttons: &[&str],
) -> i32 {
    if !state.is_open() {
        return -1;
    }
    let m = cx.theme().metrics;
    let mut chosen = None;
    let response = modal(cx, state)
        .title(title)
        .width(380.)
        .dismiss_on_backdrop(false)
        .header(false)
        .panel_role(WidgetRole::AlertDialog)
        .children(|cx| {
            let _ = text(cx, title).font_size(m.font_size + 3).weight(650);
            if !message.is_empty() {
                let _ = text(cx, message).font_size(m.font_size).color(Color::Muted);
            }
            let _ = row(cx)
                .justify(Justify::End)
                .gap(m.space(2))
                .width_percent(100.)
                .children(|cx| {
                    for (index, label) in buttons.iter().enumerate() {
                        let mut button = button(cx).text(*label).ghost();
                        if index + 1 == buttons.len() {
                            button = button.primary();
                        }
                        if button.clicked() {
                            chosen = Some(i32::try_from(index).unwrap_or(i32::MAX));
                            cx.close_modal();
                        }
                    }
                });
        });
    if let Some(index) = chosen {
        return index;
    }
    if response.dismissed {
        ALERT_DISMISSED
    } else {
        -1
    }
}
