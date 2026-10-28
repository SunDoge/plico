use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext, UiEvent};
#[derive(Default)]
struct Form {
    query: TextBuffer,
    mount: bool,
    disabled: bool,
    request: bool,
    fallback: usize,
    dialog: ModalState,
}
impl View for Form {
    fn on_unhandled_key(&mut self, _: &plico_renderer::KeyEvent) -> bool {
        self.fallback += 1;
        true
    }
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).gap(8).children(|cx| {
            if self.mount {
                text_input(cx, "Search", &self.query)
                    .autofocus(true)
                    .disabled(self.disabled)
                    .request_focus(std::mem::take(&mut self.request))
                    .navigation(true);
            }
            button(cx).text("Other");
            modal(cx, &mut self.dialog).title("Dialog").children(|cx| {
                button(cx).text("Inside");
            });
        })
    }
}
#[test]
fn autofocus_is_mount_scoped_explicit_request_and_focus_restore_are_one_shot() {
    let mut s = Source::new(Form {
        mount: true,
        ..Form::default()
    });
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    let input = s.adapter.focused().unwrap();
    let other = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Other", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Focus { target: other });
    s.build_frame(&mut tcx, 400, 300);
    assert_ne!(
        s.adapter.focused(),
        Some(input),
        "autofocus must not steal focus"
    );
    s.view.request = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(s.adapter.focused(), Some(input));
    s.handle_event(UiEvent::Focus(false));
    assert!(s.adapter.focused().is_none());
    s.handle_event(UiEvent::Focus(true));
    assert_eq!(s.adapter.focused(), Some(input));
    s.view.mount = false;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    assert!(s.adapter.focused().is_none());
    s.view.mount = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    assert!(s.adapter.focused().is_some());
    assert_ne!(
        s.adapter.focused(),
        Some(input),
        "remount gets a fresh entity"
    );
    s.view.disabled = true;
    s.view.request = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    assert!(s.adapter.focused().is_none());
}

fn key(value: &str, modifiers: plico_renderer::Modifiers) -> UiEvent {
    UiEvent::Key(plico_renderer::KeyEvent {
        phase: plico_renderer::KeyPhase::Down,
        key: value.into(),
        key_without_modifiers: value.into(),
        code: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: plico_renderer::KeyLocation::Standard,
        modifiers,
        repeat: false,
        synthetic: false,
    })
}
#[test]
fn window_key_fallback_does_not_intercept_editing_composition_or_modal_input() {
    use plico_renderer::{ImeEvent, Modifiers};
    let mut s = Source::new(Form {
        mount: true,
        ..Form::default()
    });
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    s.handle_event(UiEvent::TextInput("query".into()));
    assert!(!s.handle_event(key("r", Modifiers::empty())).handled);
    assert_eq!(s.view.fallback, 0);
    s.handle_event(key("a", Modifiers::CONTROL));
    assert_eq!(s.view.fallback, 0);
    s.handle_event(UiEvent::Ime(ImeEvent::Preedit {
        text: "input".into(),
        cursor: None,
    }));
    assert!(!s.handle_event(key("F2", Modifiers::empty())).handled);
    assert_eq!(s.view.fallback, 0);
    s.handle_event(UiEvent::Ime(ImeEvent::Disabled));
    assert!(s.handle_event(key("F2", Modifiers::empty())).handled);
    assert_eq!(s.view.fallback, 1);
    s.view.dialog.open();
    s.build_frame(&mut tcx, 400, 300);
    s.handle_event(key("F2", Modifiers::empty()));
    assert_eq!(s.view.fallback, 1);
    // Repeated requests for an input behind a modal must not break the focus trap.
    s.view.request = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    assert!(s.ime_state().is_none());
}
