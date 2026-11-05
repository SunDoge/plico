//! Ordered window-event translation; Bevy input resources are never cleared.
use crate::{PlicoClipboardRequest, PlicoUi};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyCode},
        mouse::{MouseButton, MouseScrollUnit},
        touch::TouchPhase,
    },
    prelude::*,
    window::{Ime, PrimaryWindow, WindowEvent},
};
use plico::{KeyEvent, KeyLocation, KeyPhase, Modifiers};
use plico_renderer::{
    FrameSource, GesturePhase, ImeEvent, Point, PointerButton, PointerEvent, PointerPhase, UiEvent,
    WheelDeltaMode, WheelEvent,
};
use std::collections::HashSet;

/// Read after [`crate::PlicoSystems::Input`] before applying gameplay input.
/// Raw Bevy keyboard/mouse messages remain available to other systems.
#[derive(Resource, Default, Debug)]
pub struct PlicoInput {
    /// Pointer is over an interactive UI control, or a UI drag owns the pointer.
    pub wants_pointer: bool,
    /// A UI control owns keyboard focus.
    pub wants_keyboard: bool,
    /// At least one pointer event this frame belonged to the UI.
    pub pointer_consumed: bool,
    /// At least one keyboard/text event this frame was consumed by the UI.
    pub keyboard_consumed: bool,
}
#[derive(Default)]
pub(crate) struct InputState {
    pointer: Option<Point>,
    buttons: u32,
    captured: bool,
    keys: HashSet<KeyCode>,
    composing: bool,
    #[cfg(feature = "clipboard")]
    clipboard_read: Option<(u64, bevy::clipboard::ClipboardRead)>,
}
impl InputState {
    fn modifiers(&self) -> Modifiers {
        let mut result = Modifiers::empty();
        for (keys, modifier) in [
            ([KeyCode::ShiftLeft, KeyCode::ShiftRight], Modifiers::SHIFT),
            (
                [KeyCode::ControlLeft, KeyCode::ControlRight],
                Modifiers::CONTROL,
            ),
            ([KeyCode::AltLeft, KeyCode::AltRight], Modifiers::ALT),
            ([KeyCode::SuperLeft, KeyCode::SuperRight], Modifiers::META),
        ] {
            result.set(modifier, keys.iter().any(|key| self.keys.contains(key)));
        }
        result
    }
    fn pointer_event(&self, phase: PointerPhase, button: Option<PointerButton>) -> UiEvent {
        UiEvent::Pointer(PointerEvent {
            phase,
            position: self.pointer.unwrap_or(Point { x: -1., y: -1. }),
            button,
            buttons: self.buttons,
            modifiers: self.modifiers(),
            properties: Default::default(),
        })
    }
}
fn mouse_button(button: MouseButton) -> (PointerButton, u32) {
    match button {
        MouseButton::Left => (PointerButton::Primary, 1),
        MouseButton::Right => (PointerButton::Secondary, 2),
        MouseButton::Middle => (PointerButton::Auxiliary, 4),
        MouseButton::Back => (PointerButton::Other(3), 8),
        MouseButton::Forward => (PointerButton::Other(4), 16),
        MouseButton::Other(value) => (PointerButton::Other(value), 0),
    }
}
fn key_event(event: &bevy::input::keyboard::KeyboardInput, modifiers: Modifiers) -> KeyEvent {
    let key = match &event.logical_key {
        Key::Character(text) => text.to_string(),
        Key::Space => " ".into(),
        other => format!("{other:?}"),
    };
    let code = format!("{:?}", event.key_code);
    let unmodified = code
        .strip_prefix("Key")
        .filter(|s| s.len() == 1)
        .map(str::to_ascii_lowercase)
        .or_else(|| code.strip_prefix("Digit").map(str::to_owned))
        .unwrap_or_else(|| key.clone());
    let location = match event.key_code {
        KeyCode::ShiftLeft | KeyCode::ControlLeft | KeyCode::AltLeft | KeyCode::SuperLeft => {
            KeyLocation::Left
        }
        KeyCode::ShiftRight | KeyCode::ControlRight | KeyCode::AltRight | KeyCode::SuperRight => {
            KeyLocation::Right
        }
        _ if code.starts_with("Numpad") => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    };
    KeyEvent {
        phase: if event.state == ButtonState::Pressed {
            KeyPhase::Down
        } else {
            KeyPhase::Up
        },
        key,
        key_without_modifiers: unmodified,
        code,
        location,
        modifiers,
        repeat: event.repeat,
        synthetic: false,
        text: event.text.as_ref().map(ToString::to_string),
        text_with_all_modifiers: event.text.as_ref().map(ToString::to_string),
    }
}

pub(crate) fn handle_input<V: plico::View + 'static>(
    mut ui: NonSendMut<PlicoUi<V>>,
    mut events: MessageReader<WindowEvent>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut capture: ResMut<PlicoInput>,
    mut clipboard: MessageWriter<PlicoClipboardRequest>,
) {
    *capture = PlicoInput::default();
    let Ok((window, metrics)) = windows.single() else {
        events.clear();
        return;
    };
    let ui = &mut *ui;
    for event in events.read() {
        // Flush the previous event's state changes before hit-testing or
        // delivering another key. This preserves normal shell event ordering.
        ui.source.prepare_for_event(&mut ui.text);
        let mut pending = smallvec::SmallVec::<[UiEvent; 3]>::new();
        let mut pointer = false;
        let mut keyboard = false;
        let mut release = false;
        let mut wheel = false;
        match event {
            WindowEvent::CursorMoved(event) if event.window == window => {
                ui.input.pointer = Some(Point {
                    x: event.position.x as f64,
                    y: event.position.y as f64,
                });
                pending.push(ui.input.pointer_event(PointerPhase::Move, None));
                pointer = true;
            }
            WindowEvent::CursorEntered(event) if event.window == window => {
                pending.push(ui.input.pointer_event(PointerPhase::Enter, None));
                pointer = true;
            }
            WindowEvent::CursorLeft(event) if event.window == window => {
                pending.push(ui.input.pointer_event(PointerPhase::Leave, None));
                ui.input.pointer = None;
                release = true;
            }
            WindowEvent::MouseButtonInput(event) if event.window == window => {
                let (button, bit) = mouse_button(event.button);
                let down = event.state == ButtonState::Pressed;
                if down {
                    ui.input.buttons |= bit;
                } else {
                    ui.input.buttons &= !bit;
                }
                if down
                    && ui
                        .input
                        .pointer
                        .is_some_and(|point| ui.source.hit_test(point).is_some())
                {
                    ui.input.captured = true;
                }
                pending.push(ui.input.pointer_event(
                    if down {
                        PointerPhase::Down
                    } else {
                        PointerPhase::Up
                    },
                    Some(button),
                ));
                pointer = true;
                release = !down && ui.input.buttons == 0;
            }
            WindowEvent::MouseWheel(event) if event.window == window => {
                wheel = true;
                let line = event.unit == MouseScrollUnit::Line;
                let factor = if line {
                    plico_renderer::WHEEL_LINE_DELTA
                } else {
                    1. / f64::from(metrics.scale_factor())
                };
                pending.push(UiEvent::Wheel(WheelEvent {
                    position: ui.input.pointer.unwrap_or(Point { x: -1., y: -1. }),
                    delta_x: -(event.x as f64) * factor,
                    delta_y: -(event.y as f64) * factor,
                    delta_mode: if line {
                        WheelDeltaMode::Line
                    } else {
                        WheelDeltaMode::Pixel
                    },
                    phase: match event.phase {
                        TouchPhase::Started => GesturePhase::Started,
                        TouchPhase::Moved => GesturePhase::Changed,
                        TouchPhase::Ended => GesturePhase::Ended,
                        TouchPhase::Canceled => GesturePhase::Cancelled,
                    },
                    modifiers: ui.input.modifiers(),
                }));
                pointer = true;
            }
            WindowEvent::KeyboardInput(event) if event.window == window => {
                if event.state == ButtonState::Pressed {
                    ui.input.keys.insert(event.key_code);
                } else {
                    ui.input.keys.remove(&event.key_code);
                }
                let key = key_event(event, ui.input.modifiers());
                pending.push(UiEvent::ModifiersChanged(key.modifiers));
                pending.push(UiEvent::Key(key));
                keyboard = true;
            }
            WindowEvent::Ime(ime) => {
                let (target, event) = match ime {
                    Ime::Enabled { window } => (*window, ImeEvent::Enabled),
                    Ime::Disabled { window } => (*window, ImeEvent::Disabled),
                    Ime::Preedit {
                        window,
                        value,
                        cursor,
                    } => (
                        *window,
                        ImeEvent::Preedit {
                            text: value.clone(),
                            cursor: *cursor,
                        },
                    ),
                    Ime::Commit { window, value } => (*window, ImeEvent::Commit(value.clone())),
                };
                if target != window {
                    continue;
                }
                ui.input.composing =
                    matches!(&event, ImeEvent::Preedit { text, .. } if !text.is_empty());
                pending.push(UiEvent::Ime(event));
                keyboard = true;
            }
            WindowEvent::WindowFocused(event) if event.window == window => {
                if !event.focused {
                    ui.input = InputState::default();
                }
                pending.push(UiEvent::Focus(event.focused));
            }
            _ => continue,
        }
        let over_control = ui
            .input
            .pointer
            .is_some_and(|point| ui.source.hit_test(point).is_some());
        for event in pending {
            let key_text = if let UiEvent::Key(key) = &event {
                (key.phase == KeyPhase::Down
                    && !key.modifiers.control()
                    && !key.modifiers.meta()
                    && !ui.input.composing)
                    .then(|| key.text.clone())
                    .flatten()
            } else {
                None
            };
            let response = ui.send_event(event);
            capture.pointer_consumed |=
                pointer && (over_control || ui.input.captured || wheel && response.handled);
            capture.keyboard_consumed |= keyboard && response.handled;
            if let Some(request) = response.clipboard {
                clipboard.write(PlicoClipboardRequest(request));
            }
            if !response.consume_key_text
                && let Some(text) = key_text
                && !text.chars().any(char::is_control)
            {
                let response = ui.send_event(UiEvent::TextInput(text));
                capture.keyboard_consumed |= response.handled;
            }
        }
        if release {
            ui.input.captured = false;
        }
    }
    capture.wants_pointer = ui.input.captured
        || ui
            .input
            .pointer
            .is_some_and(|point| ui.source.hit_test(point).is_some());
    capture.wants_keyboard = ui.source.adapter.focused().is_some();
}

#[cfg(feature = "clipboard")]
pub(crate) fn handle_clipboard<V: plico::View + 'static>(
    mut ui: NonSendMut<PlicoUi<V>>,
    mut requests: MessageReader<PlicoClipboardRequest>,
    mut clipboard: ResMut<bevy::clipboard::Clipboard>,
) {
    for request in requests.read() {
        match &request.0 {
            plico_renderer::ClipboardRequest::Read => {
                if let Some(client) = ui.source.ime_state().map(|state| state.client_id) {
                    ui.input.clipboard_read = Some((client, clipboard.fetch_text()));
                }
            }
            plico_renderer::ClipboardRequest::Write(text) => {
                if let Err(error) = clipboard.set_text(text.as_str()) {
                    tracing::warn!(%error, "Plico Bevy clipboard write failed");
                }
            }
        }
    }
    if let Some((client, read)) = &mut ui.input.clipboard_read
        && let Some(result) = read.poll_result()
    {
        let client = *client;
        ui.input.clipboard_read = None;
        match result {
            Ok(text)
                if ui
                    .source
                    .ime_state()
                    .is_some_and(|state| state.client_id == client) =>
            {
                ui.send_event(UiEvent::Paste(text));
            }
            Err(error) => tracing::warn!(%error, "Plico Bevy clipboard read failed"),
            _ => {}
        }
    }
}
