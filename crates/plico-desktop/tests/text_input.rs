use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, ImeEvent, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction,
    SemanticRole, TextContext, UiEvent, WidgetImePurpose,
};
#[derive(Default)]
struct Form {
    ordinary: TextBuffer,
    secret: Option<TextBuffer>,
    submitted: bool,
}
impl HostView for Form {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let secret = self.secret.get_or_insert_with(TextBuffer::secret);
        column(cx).gap(8).children(|cx| {
            text_input(cx, "Email", &self.ordinary);
            if text_input(cx, "Password", secret).response().submitted {
                self.submitted = true;
            }
        })
    }
}
fn key(value: &str, modifiers: Modifiers) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        code: value.into(),
        key_without_modifiers: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers,
        repeat: false,
        synthetic: false,
    })
}
fn focus(s: &mut Source<Form>, label: &str) {
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::TextInput, label, None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Focus { target: id }));
}
#[test]
fn native_events_mask_secret_ime_and_clipboard_and_submit_from_enter() {
    let mut s = Source::new(Form::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    focus(&mut s, "Password");
    assert!(
        s.handle_event(UiEvent::TextInput("🌱synthetic-input".into()))
            .handled
    );
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(s.ime_state().unwrap().purpose, WidgetImePurpose::Password);
    assert_eq!(s.ime_state().unwrap().surrounding_text, "");
    assert!(
        !serde_json::to_string(&s.engine.snapshot())
            .unwrap()
            .contains("synthetic-input")
    );
    assert!(s.adapter.placed.iter().all(|p| p.paint.text.is_none()));
    s.handle_event(key("a", Modifiers::CONTROL));
    assert!(
        s.handle_event(key("c", Modifiers::CONTROL))
            .clipboard
            .is_none()
    );
    s.handle_event(UiEvent::Paste("替换".into()));
    s.handle_event(UiEvent::Ime(ImeEvent::Preedit {
        text: "组合".into(),
        cursor: None,
    }));
    assert_eq!(s.view.secret.as_ref().unwrap().presentation().text, "••••");
    s.handle_event(UiEvent::Ime(ImeEvent::Commit("确认".into())));
    assert!(
        s.handle_event(key("Enter", Modifiers::empty()))
            .consume_key_text
    );
    s.build_frame(&mut tcx, 400, 300);
    assert!(s.view.submitted);
    focus(&mut s, "Email");
    s.handle_event(UiEvent::TextInput("hello@example.invalid".into()));
    let response = s.handle_event(key(" ", Modifiers::empty()));
    assert!(!response.consume_key_text);
    s.handle_event(UiEvent::TextInput(" ".into()));
    assert_eq!(
        s.view.ordinary.text().as_deref(),
        Some("hello@example.invalid ")
    );
}
