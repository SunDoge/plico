use plico::prelude::*;
use plico::{KeyEvent, KeyLocation, KeyPhase, Modifiers};
use plico_runtime::{
    HostView,
    testing::{Command, Driver},
};

struct FindDemo {
    open: bool,
    query: String,
    current: usize,
    moves: usize,
}

impl HostView for FindDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        if find_bar(cx, &mut self.open, &mut self.query, 3, &mut self.current).changed() {
            self.moves += 1;
        }
    }
}

fn key(key: &str, modifiers: Modifiers) -> Command {
    Command::KeyEvent {
        event: KeyEvent {
            phase: KeyPhase::Down,
            key: key.into(),
            key_without_modifiers: key.into(),
            code: if key.eq_ignore_ascii_case("g") {
                "KeyG".into()
            } else {
                key.into()
            },
            text: None,
            text_with_all_modifiers: None,
            location: KeyLocation::Standard,
            modifiers,
            repeat: false,
            synthetic: false,
        },
    }
}

#[test]
fn find_bar_selects_the_old_query_and_routes_native_find_shortcuts() {
    let mut ui = Driver::new(
        FindDemo {
            open: true,
            query: "old query".into(),
            current: 0,
            moves: 0,
        },
        640,
        120,
    )
    .unwrap();

    ui.execute(Command::Text { text: "new".into() }).unwrap();
    assert_eq!(ui.view().query, "new", "opening selects the old query");
    assert!(
        ui.snapshot().exposed_nodes().iter().any(|node| {
            node.role.as_str() == "status" && node.label.as_deref() == Some("1 of 3")
        }),
        "the changing match count is a live status"
    );

    ui.execute(Command::Key {
        key: "Enter".into(),
    })
    .unwrap();
    assert_eq!(ui.view().current, 1);
    ui.execute(key("Enter", Modifiers::SHIFT)).unwrap();
    assert_eq!(ui.view().current, 0);

    ui.execute(key("F3", Modifiers::empty())).unwrap();
    assert_eq!(ui.view().current, 1);
    ui.execute(key("F3", Modifiers::SHIFT)).unwrap();
    assert_eq!(ui.view().current, 0);

    let primary = if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    };
    ui.execute(key("g", primary)).unwrap();
    assert_eq!(ui.view().current, 1);
    ui.execute(key("g", primary | Modifiers::SHIFT)).unwrap();
    assert_eq!(ui.view().current, 0);
    assert_eq!(ui.view().moves, 6);

    ui.execute(key("Escape", Modifiers::empty())).unwrap();
    assert!(!ui.view().open);
}
