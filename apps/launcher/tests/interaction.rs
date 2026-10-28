use launcher::{Category, Launcher};
use plico_desktop::{HostAction, Source, WindowCommand};
use plico_renderer::{
    FrameSource, ImeEvent, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction,
    SemanticRole, TextContext, UiEvent,
};
fn key(value: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        code: value.into(),
        key_without_modifiers: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
fn frame(s: &mut Source<Launcher>, tcx: &mut TextContext) {
    s.build_frame(tcx, 680, 526);
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
}
#[test]
fn autofocus_navigation_reveal_and_activation_keep_search_focus() {
    let mut s = Source::new(Launcher::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let focus = s.adapter.focused();
    assert!(focus.is_some());
    assert!(s.ime_state().is_some());
    for _ in 0..8 {
        assert!(s.handle_event(key("ArrowDown")).consume_key_text);
        frame(&mut s, &mut tcx);
        assert_eq!(s.adapter.focused(), focus);
    }
    assert_eq!(s.view.selected_id(), Some("theme"));
    let port = s
        .adapter
        .placed
        .iter()
        .find(|n| n.scroll.scrollable[1])
        .unwrap();
    assert!(port.scroll.offset[1] > 0.);
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "切换明暗主题", None)
        .unwrap()
        .id;
    assert!(s.view.dark);
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert!(!s.view.dark);
    assert_eq!(s.adapter.focused(), focus);
    assert!(s.view.feedback().contains("已切换"));
    // Accessibility/mouse-equivalent click follows the same action path.
    s.handle_semantic_action(SemanticAction::Click { target });
    frame(&mut s, &mut tcx);
    assert!(s.view.dark);
}
#[test]
fn search_empty_recovery_and_escape_close_are_native_actions() {
    let mut s = Source::new(Launcher::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    s.handle_event(UiEvent::TextInput("找不到这个应用".into()));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.result_count(), 0);
    assert_eq!(s.view.selected_id(), None);
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert!(s.take_host_action().is_none());
    s.handle_event(key("Escape"));
    frame(&mut s, &mut tcx);
    assert!(s.view.query.is_empty());
    assert!(s.view.result_count() > 0);
    assert!(s.take_host_action().is_none());
    s.handle_event(key("Escape"));
    frame(&mut s, &mut tcx);
    assert_eq!(
        s.take_host_action(),
        Some(HostAction::Window(WindowCommand::Close))
    );
    assert!(s.take_host_action().is_none());
}
#[test]
fn ime_owns_arrows_enter_and_first_escape_while_composing() {
    let mut s = Source::new(Launcher::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    s.handle_event(UiEvent::Ime(ImeEvent::Preedit {
        text: "zhong".into(),
        cursor: None,
    }));
    assert!(!s.handle_event(key("ArrowDown")).consume_key_text);
    s.handle_event(key("Enter"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.selected_id(), Some("code"));
    assert!(s.view.feedback().is_empty());
    s.handle_event(key("Escape"));
    frame(&mut s, &mut tcx);
    assert!(!s.view.query.is_composing());
    assert!(s.take_host_action().is_none());
    s.handle_event(UiEvent::Ime(ImeEvent::Commit("终端".into())));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.selected_id(), Some("terminal"));
}
#[test]
fn filter_change_and_case_insensitive_multi_token_search_reset_selection() {
    let mut s = Source::new(Launcher::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    s.handle_event(UiEvent::TextInput("VISUAL code".into()));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.result_count(), 1);
    assert_eq!(s.view.selected_id(), Some("code"));
    s.view.query.clear();
    s.view.category = Category::Commands;
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.result_count(), 3);
    assert_eq!(s.view.selected_id(), Some("theme"));
    s.handle_event(key("PageDown"));
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.selected_id(), Some("close"));
}
