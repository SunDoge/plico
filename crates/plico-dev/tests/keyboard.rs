use plico_core::{KeyEvent, KeyLocation, KeyPhase, Modifiers, Shortcut, UiContext};
use plico_dev::{
    App, Request,
    services::{ServiceApp, Services},
};
use serde::{Deserialize, Serialize};
fn key() -> KeyEvent {
    KeyEvent {
        phase: KeyPhase::Down,
        key: "k".into(),
        key_without_modifiers: "k".into(),
        code: "KeyK".into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        },
        repeat: false,
        synthetic: false,
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Demo {
    count: u32,
}
impl App for Demo {
    fn show(&mut self, _: &mut UiContext) {}
    fn on_key(&mut self, key: &str) -> bool {
        if key == "F5" {
            self.count += 10;
            true
        } else {
            false
        }
    }
    fn on_key_event(&mut self, key: &KeyEvent) -> bool {
        if Shortcut::primary("k").matches(key) {
            self.count += 1;
            true
        } else if key.modifiers.is_empty() {
            self.on_key(&key.key)
        } else {
            false
        }
    }
}
#[test]
fn full_events_cross_the_module_protocol_and_legacy_keys_still_work() {
    let request = Request {
        key_events: vec![key()],
        keys: vec!["F5".into()],
        ..Default::default()
    };
    let wire = serde_json::to_vec(&request).unwrap();
    let frame = plico_dev::render::<Demo>(serde_json::from_slice(&wire).unwrap()).unwrap();
    assert!(frame.key_handled);
    assert_eq!(frame.state.decode::<Demo>().unwrap().count, 11);
    let mut up = key();
    up.phase = KeyPhase::Up;
    let frame = plico_dev::render::<Demo>(Request {
        key_events: vec![up],
        ..Default::default()
    })
    .unwrap();
    assert!(!frame.key_handled);
}
#[derive(Default, Serialize, Deserialize)]
struct ServiceDemo;
impl ServiceApp for ServiceDemo {
    type Snapshot = u32;
    type Command = u32;
    fn show(&mut self, _: &mut UiContext, _: &mut Services<u32, u32>) {}
    fn on_key_event(&mut self, key: &KeyEvent, services: &mut Services<u32, u32>) -> bool {
        if Shortcut::primary("k").matches(key) {
            services.send(*services.snapshot() + 1);
            true
        } else {
            false
        }
    }
}
#[test]
fn service_combinations_publish_one_typed_command() {
    let frame = plico_dev::services::render::<ServiceDemo>(Request {
        services: serde_json::json!(41),
        key_events: vec![key()],
        ..Default::default()
    })
    .unwrap();
    assert!(frame.key_handled);
    assert_eq!(frame.commands, vec![serde_json::json!(42)]);
}
