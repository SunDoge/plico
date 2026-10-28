use plico::{Engine, InputEdit, Kind, prelude::*};

#[test]
fn unicode_selection_editing_and_secret_presentation() {
    let buffer = TextBuffer::secret();
    buffer.edit(InputEdit::Insert("a🌱e\u{301}".to_owned().into()));
    assert!(buffer.text().is_none());
    assert_eq!(buffer.presentation().text, "•••");
    buffer.edit(InputEdit::Backspace);
    assert_eq!(buffer.presentation().text, "••");
    buffer.edit(InputEdit::Left(false));
    buffer.edit(InputEdit::Delete);
    assert_eq!(buffer.presentation().text, "•");
    buffer.edit(InputEdit::Preedit("测试".to_owned().into()));
    assert_eq!(buffer.presentation().text, "•••");
    buffer.edit(InputEdit::Insert("确认".to_owned().into()));
    assert_eq!(&*buffer.take(), "a确认");
    assert!(buffer.is_empty());
    assert_eq!(buffer.presentation().text, "");
    let ordinary = TextBuffer::new();
    ordinary.set("hello world".to_owned().into());
    ordinary.edit(InputEdit::Home(false));
    ordinary.edit(InputEdit::Right(true));
    assert_eq!(ordinary.selected_text().as_deref(), Some("h"));
    ordinary.edit(InputEdit::Insert("H\n".to_owned().into()));
    assert_eq!(ordinary.text().as_deref(), Some("Hello world"));
}
#[test]
fn editor_value_never_enters_declarations_or_reload_snapshots() {
    let mut engine = Engine::default();
    let buffer = TextBuffer::secret();
    let mut response = InputResponse::default();
    #[component]
    fn form(cx: &mut UiContext, buffer: &TextBuffer, response: &mut InputResponse) {
        *response = text_input(cx, "Password", buffer)
            .placeholder("Enter password")
            .response();
    }
    engine
        .frame_declarations(|cx| form(cx, &buffer, &mut response))
        .unwrap();
    buffer.edit(InputEdit::Insert(
        "🌱private-synthetic-input".to_owned().into(),
    ));
    buffer.edit(InputEdit::Submit);
    engine
        .frame_declarations(|cx| form(cx, &buffer, &mut response))
        .unwrap();
    assert!(response.changed && response.submitted);
    let snapshot = engine.snapshot();
    assert_eq!(snapshot[0].kind, Kind::TextInput);
    assert_eq!(snapshot[0].text, "Password");
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("synthetic-input")
    );
    engine
        .frame_declarations(|cx| form(cx, &buffer, &mut response))
        .unwrap();
    assert_eq!(response, InputResponse::default());
}

#[test]
fn editor_replacement_and_placeholder_changes_invalidate_paint_without_layout() {
    let mut engine = Engine::default();
    let first = TextBuffer::new();
    let second = TextBuffer::secret();
    assert_ne!(first.revision(), second.revision());
    #[component]
    fn form(cx: &mut UiContext, buffer: &TextBuffer, hint: &str) {
        text_input(cx, "Editor", buffer).placeholder(hint);
    }
    engine
        .frame_declarations(|cx| form(cx, &first, "First"))
        .unwrap();
    engine
        .frame_declarations(|cx| form(cx, &first, "Second"))
        .unwrap();
    assert!(engine.changes().contains(plico::Changes::PAINT));
    assert!(!engine.changes().contains(plico::Changes::LAYOUT));
    engine
        .frame_declarations(|cx| form(cx, &second, "Second"))
        .unwrap();
    assert!(engine.changes().contains(plico::Changes::PAINT));
    assert!(!engine.changes().contains(plico::Changes::LAYOUT));
    let entity = engine.node(engine.root()).unwrap().children[0];
    assert!(engine.input(entity).unwrap().0.is_secret());
}
