use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};
#[derive(Default)]
struct Demo {
    select_state: PopoverState,
    menu_state: PopoverState,
    value: u8,
    changes: usize,
    actions: usize,
    reverse: bool,
    empty: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let mut options = vec![
            SelectOption::new(10u64, 0, "Stable"),
            SelectOption::new(20u64, 1, "Beta").disabled(true),
            SelectOption::new(30u64, 2, "Nightly"),
        ];
        if self.reverse {
            options.reverse();
        }
        if self.empty {
            options.clear();
        }
        column(cx)
            .padding(12)
            .gap(12)
            .align(Align::Start)
            .children(|cx| {
                if select(
                    cx,
                    &mut self.select_state,
                    "Release channel",
                    &mut self.value,
                    &options,
                )
                .motion(Motion::None)
                .response()
                .changed
                {
                    self.changes += 1;
                }
                menu(cx, &mut self.menu_state, "Actions")
                    .motion(Motion::None)
                    .children(|cx| {
                        menu_item(cx, "Unavailable").disabled(true);
                        if menu_item(cx, "Duplicate").clicked() {
                            self.actions += 1;
                        }
                        separator(cx);
                        if menu_item(cx, "Archive").clicked() {
                            self.actions += 10;
                        }
                    });
                button(cx).text("After");
            })
    }
}
fn key(name: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: name.into(),
        code: name.into(),
        key_without_modifiers: name.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
fn id(s: &Source<Demo>, role: SemanticRole, label: &str) -> u64 {
    s.adapter
        .semantics(&s.engine, 0)
        .node_by_role(role, label, None)
        .unwrap()
        .id
}
fn focus(s: &mut Source<Demo>, role: SemanticRole, label: &str) {
    let target = id(s, role, label);
    assert!(s.handle_semantic_action(SemanticAction::Focus { target }));
}
fn frame(s: &mut Source<Demo>, tcx: &mut TextContext) {
    s.build_frame(tcx, 600, 400);
    assert!(s.last_error.is_none());
}
#[test]
fn select_keyboard_skips_disabled_cancels_then_commits_once() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let trigger = id(&s, SemanticRole::ComboBox, "Release channel");
    assert_eq!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::ComboBox, "Release channel", None)
            .unwrap()
            .value
            .as_deref(),
        Some("Stable")
    );
    focus(&mut s, SemanticRole::ComboBox, "Release channel");
    s.handle_event(key("ArrowDown"));
    frame(&mut s, &mut tcx);
    assert!(s.view.select_state.is_open());
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(id(&s, SemanticRole::Option, "Stable"))
    );
    s.handle_event(key("ArrowDown"));
    frame(&mut s, &mut tcx);
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(id(&s, SemanticRole::Option, "Nightly"))
    );
    s.handle_event(key("Escape"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.value, 0);
    assert_eq!(s.view.changes, 0);
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(trigger));
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    s.handle_event(key("End"));
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.value, 2);
    assert_eq!(s.view.changes, 1);
    assert!(!s.view.select_state.is_open());
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.changes, 1);
}
#[test]
fn option_keys_preserve_focus_across_reorder_and_first_letter_search() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    focus(&mut s, SemanticRole::ComboBox, "Release channel");
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    s.handle_event(key("n"));
    frame(&mut s, &mut tcx);
    let nightly = id(&s, SemanticRole::Option, "Nightly");
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(nightly));
    s.view.reverse = true;
    s.request_rebuild();
    frame(&mut s, &mut tcx);
    assert_eq!(id(&s, SemanticRole::Option, "Nightly"), nightly);
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(nightly));
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.value, 2);
}
#[test]
fn menu_navigates_commands_and_closes_after_activation() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    focus(&mut s, SemanticRole::Button, "Actions");
    s.handle_event(key("ArrowDown"));
    frame(&mut s, &mut tcx);
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Menu, "Actions", None)
            .is_some()
    );
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(id(&s, SemanticRole::MenuItem, "Duplicate"))
    );
    s.handle_event(key("End"));
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.actions, 10);
    assert!(!s.view.menu_state.is_open());
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(id(&s, SemanticRole::Button, "Actions"))
    );
}
#[test]
fn empty_options_disable_trigger_and_unknown_value_is_preserved() {
    let mut s = Source::new(Demo {
        value: 99,
        empty: true,
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let target = id(&s, SemanticRole::ComboBox, "Release channel");
    assert!(!s.handle_semantic_action(SemanticAction::Click { target }));
    assert_eq!(s.view.value, 99);
    s.view.empty = false;
    s.request_rebuild();
    frame(&mut s, &mut tcx);
    focus(&mut s, SemanticRole::ComboBox, "Release channel");
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.value, 99);
    assert_eq!(s.view.changes, 0);
}

#[derive(Default)]
struct LongList {
    popup: PopoverState,
    value: usize,
}
impl View for LongList {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let labels: Vec<_> = (0..80).map(|i| format!("Option {i}")).collect();
        let options: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| SelectOption::new(i, i, label))
            .collect();
        column(cx).padding(12).children(|cx| {
            select(cx, &mut self.popup, "Long list", &mut self.value, &options)
                .motion(Motion::None);
        })
    }
}
#[test]
fn selected_option_and_keyboard_navigation_scroll_into_the_constrained_panel() {
    let mut s = Source::new(LongList {
        value: 70,
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 900);
    let trigger = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::ComboBox, "Long list", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Click { target: trigger });
    s.build_frame(&mut tcx, 400, 900);
    let roomy = s.adapter.semantics(&s.engine, 0);
    let panel = roomy
        .node_by_role(SemanticRole::ListBox, "Long list", None)
        .unwrap();
    assert!(panel.bounds[3] - panel.bounds[1] <= 280.);
    s.build_frame(&mut tcx, 400, 260);
    let snapshot = s.adapter.semantics(&s.engine, 0);
    let chosen = snapshot
        .node_by_role(SemanticRole::Option, "Option 70", None)
        .unwrap();
    let panel = snapshot
        .node_by_role(SemanticRole::ListBox, "Long list", None)
        .unwrap();
    assert_eq!(snapshot.focus, Some(chosen.id));
    assert!(chosen.bounds[1] >= panel.bounds[1] && chosen.bounds[3] <= panel.bounds[3]);
    assert!(panel.bounds[3] <= 252.);
    s.handle_event(key("Home"));
    s.build_frame(&mut tcx, 400, 260);
    let snapshot = s.adapter.semantics(&s.engine, 0);
    let first = snapshot
        .node_by_role(SemanticRole::Option, "Option 0", None)
        .unwrap();
    assert_eq!(snapshot.focus, Some(first.id));
    assert!(first.bounds[1] >= panel.bounds[1]);
    s.handle_event(key("Enter"));
    s.build_frame(&mut tcx, 400, 260);
    assert_eq!(s.view.value, 0);
}
#[test]
fn arrow_up_opens_menu_on_last_enabled_command_and_up_wraps() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    focus(&mut s, SemanticRole::Button, "Actions");
    s.handle_event(key("ArrowUp"));
    frame(&mut s, &mut tcx);
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(id(&s, SemanticRole::MenuItem, "Archive"))
    );
    s.handle_event(key("Home"));
    s.handle_event(key("ArrowUp"));
    frame(&mut s, &mut tcx);
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(id(&s, SemanticRole::MenuItem, "Archive"))
    );
}

#[test]
fn pointer_highlight_can_be_activated_by_keyboard_without_changing_value_on_hover() {
    use plico_renderer::{Point, PointerEvent, PointerPhase};
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    focus(&mut s, SemanticRole::ComboBox, "Release channel");
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    let bounds = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Option, "Nightly", None)
        .unwrap()
        .bounds;
    s.handle_event(UiEvent::Pointer(PointerEvent {
        phase: PointerPhase::Move,
        position: Point {
            x: (bounds[0] + bounds[2]) as f64 / 2.,
            y: (bounds[1] + bounds[3]) as f64 / 2.,
        },
        button: None,
        buttons: 0,
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    }));
    assert_eq!(s.view.value, 0);
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(id(&s, SemanticRole::Option, "Nightly"))
    );
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.value, 2);
    assert_eq!(s.view.changes, 1);
}
