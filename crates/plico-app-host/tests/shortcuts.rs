use plico::{KeyEvent, KeyLocation, KeyPhase, Modifiers, Shortcut, prelude::*};
use plico_app_host::{NativeApp, ServiceView};
use plico_dev::App;
use plico_renderer::{ImeEvent, UiEvent};
use plico_runtime::testing::{Command, Driver};
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Demo {
    count: u32,
    #[serde(skip)]
    input: TextBuffer,
}

impl App for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        text_input(cx, "Query", &self.input);
    }

    fn on_key_event(&mut self, event: &KeyEvent) -> bool {
        if Shortcut::primary("k").matches(event) || Shortcut::primary("c").matches(event) {
            self.count += 1;
            true
        } else {
            false
        }
    }
}

fn key(name: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: name.into(),
        key_without_modifiers: name.into(),
        code: String::new(),
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
    })
}

#[test]
fn native_app_receives_combinations_but_text_editing_and_ime_keep_priority() {
    let mut driver = Driver::new(NativeApp(Demo::default()), 400, 300).unwrap();
    assert!(driver.dispatch_event(key("k")).unwrap().handled);
    assert_eq!(driver.view().0.count, 1);
    driver
        .execute(Command::Focus {
            role: "textbox".into(),
            label: "Query".into(),
            occurrence: None,
        })
        .unwrap();
    assert!(driver.dispatch_event(key("k")).unwrap().handled);
    assert_eq!(driver.view().0.count, 2);
    driver.dispatch_event(key("c")).unwrap();
    assert_eq!(driver.view().0.count, 2);
    driver
        .dispatch_event(UiEvent::Ime(ImeEvent::Preedit {
            text: "あ".into(),
            cursor: None,
        }))
        .unwrap();
    driver.dispatch_event(key("k")).unwrap();
    assert_eq!(driver.view().0.count, 2);
}

#[derive(Default, Serialize, Deserialize)]
struct ServiceDemo;

impl plico_dev::services::ServiceApp for ServiceDemo {
    type Snapshot = u32;
    type Command = u32;

    fn show(&mut self, _: &mut UiContext, _: &mut plico_dev::services::Services<u32, u32>) {}

    fn on_key_event(
        &mut self,
        event: &KeyEvent,
        services: &mut plico_dev::services::Services<u32, u32>,
    ) -> bool {
        if Shortcut::primary("k").matches(event) {
            services.send(*services.snapshot() + 1);
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
struct Host {
    commands: Vec<u32>,
}

impl plico_dev::services::HostService for Host {
    type Snapshot = u32;
    type Command = u32;

    fn snapshot(&self) -> u32 {
        41
    }

    fn send(&mut self, command: u32) {
        self.commands.push(command);
    }
}

#[test]
fn static_services_and_full_testing_commands_use_the_same_hook() {
    let view = ServiceView::new(ServiceDemo, Host::default());
    let mut driver = Driver::new(view, 400, 300).unwrap();
    let UiEvent::Key(event) = key("k") else {
        unreachable!()
    };
    let command = Command::KeyEvent { event };
    let wire = serde_json::to_string(&command).unwrap();
    driver
        .execute(serde_json::from_str(&wire).unwrap())
        .unwrap();
    assert_eq!(driver.view().services.commands, [42]);
}

struct ConstructedApp {
    label: &'static str,
}

impl App for ConstructedApp {
    fn show(&mut self, cx: &mut UiContext<'_>) {
        text(cx, self.label);
    }
}

struct ConstructedServiceApp {
    label: &'static str,
}

struct ConstructedSnapshot(&'static str);
struct ConstructedCommand;
struct ConstructedHost;

impl plico_dev::services::HostService for ConstructedHost {
    type Snapshot = ConstructedSnapshot;
    type Command = ConstructedCommand;

    fn snapshot(&self) -> Self::Snapshot {
        ConstructedSnapshot("local snapshot")
    }

    fn send(&mut self, _: Self::Command) {}
}

impl plico_dev::services::ServiceApp for ConstructedServiceApp {
    type Snapshot = ConstructedSnapshot;
    type Command = ConstructedCommand;

    fn show(
        &mut self,
        cx: &mut UiContext<'_>,
        services: &mut plico_dev::services::Services<ConstructedSnapshot, ConstructedCommand>,
    ) {
        text(cx, self.label);
        text(cx, services.snapshot().0);
    }
}

#[test]
fn statically_constructed_apps_do_not_require_default_or_serialization() {
    let plain = Driver::new(NativeApp(ConstructedApp { label: "plain" }), 400, 300).unwrap();
    assert!(
        plain
            .snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("plain"))
    );

    let services = Driver::new(
        ServiceView::new(ConstructedServiceApp { label: "services" }, ConstructedHost),
        400,
        300,
    )
    .unwrap();
    assert!(
        services
            .snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("services"))
    );
    assert!(
        services
            .snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("local snapshot"))
    );
}
