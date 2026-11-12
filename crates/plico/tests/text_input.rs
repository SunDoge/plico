use plico::{Engine, InputEdit, Kind, prelude::*};
use zeroize::Zeroizing;

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
    assert!(response.changed() && response.submitted());
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

#[test]
fn string_binding_writes_user_edits_and_preserves_editor_state() {
    let mut engine = Engine::default();
    let mut value = "hello".to_owned();
    let mut response = InputResponse::default();
    #[component]
    fn form(cx: &mut UiContext, value: &mut String, response: &mut InputResponse) {
        *response = text_input_string(cx, "Path", value).response();
    }
    engine
        .frame_declarations(|cx| form(cx, &mut value, &mut response))
        .unwrap();
    let entity = engine.node(engine.root()).unwrap().children[0];
    let buffer = engine.input(entity).unwrap().0.clone();
    buffer.edit(InputEdit::Home(false));
    buffer.edit(InputEdit::Right(true));
    buffer.edit(InputEdit::Preedit("拼".to_owned().into()));
    engine
        .frame_declarations(|cx| form(cx, &mut value, &mut response))
        .unwrap();
    assert_eq!(value, "hello");
    assert!(!response.changed());
    assert!(buffer.is_composing());
    assert_eq!(buffer.selection_bytes(), (1, 0));

    buffer.edit(InputEdit::Insert("H".to_owned().into()));
    engine
        .frame_declarations(|cx| form(cx, &mut value, &mut response))
        .unwrap();
    assert_eq!(value, "Hello");
    assert!(response.changed());
    assert!(!buffer.is_composing());
    engine
        .frame_declarations(|cx| form(cx, &mut value, &mut response))
        .unwrap();
    assert_eq!(response, InputResponse::default());
}

#[test]
fn string_binding_external_updates_do_not_look_like_user_edits() {
    let mut engine = Engine::default();
    let mut value = "initial".to_owned();
    #[component]
    fn form(cx: &mut UiContext, value: &mut String) -> InputResponse {
        text_input_string(cx, "Path", value).response()
    }
    let frame = |engine: &mut Engine, value: &mut String| {
        let mut response = InputResponse::default();
        engine
            .frame_declarations(|cx| response = form(cx, value))
            .unwrap();
        response
    };
    assert_eq!(frame(&mut engine, &mut value), InputResponse::default());
    value = "external".into();
    assert_eq!(frame(&mut engine, &mut value), InputResponse::default());
    let entity = engine.node(engine.root()).unwrap().children[0];
    assert_eq!(
        engine.input(entity).unwrap().0.text().as_deref(),
        Some("external")
    );
    assert_eq!(frame(&mut engine, &mut value), InputResponse::default());
}

#[test]
fn string_binding_drop_commits_and_disabled_edits_are_discarded() {
    let mut engine = Engine::default();
    let mut value = "hello".to_owned();
    #[component]
    fn form(cx: &mut UiContext, value: &mut String, disabled: bool) {
        column(cx).disabled(disabled).children(|cx| {
            cx.text_input_string("Path", value);
        });
    }
    engine
        .frame_declarations(|cx| form(cx, &mut value, false))
        .unwrap();
    let entity = engine.node(engine.root()).unwrap().children[0];
    let input = engine.node(entity).unwrap().children[0];
    let buffer = engine.input(input).unwrap().0.clone();
    buffer.edit(InputEdit::End(false));
    buffer.edit(InputEdit::Insert("!".to_owned().into()));
    engine
        .frame_declarations(|cx| form(cx, &mut value, false))
        .unwrap();
    assert_eq!(value, "hello!");

    buffer.edit(InputEdit::Insert("ignored".to_owned().into()));
    engine
        .frame_declarations(|cx| form(cx, &mut value, true))
        .unwrap();
    assert_eq!(value, "hello!");
    engine
        .frame_declarations(|cx| form(cx, &mut value, false))
        .unwrap();
    assert_eq!(value, "hello!");
    assert_eq!(buffer.text().as_deref(), Some("hello!"));
}

#[test]
fn text_input_keeps_its_fill_and_leaves_hover_to_the_border() {
    let mut engine = Engine::default();
    let buffer = TextBuffer::new();
    engine
        .frame_declarations(|cx| {
            text_input(cx, "Name", &buffer);
        })
        .unwrap();
    let input = engine.node(engine.root()).unwrap().children[0];
    let style = engine.node(input).unwrap().style;
    assert_eq!(style.background, Some(Color::Control));
    assert_eq!(style.hover_background, None);
    assert_eq!(style.pressed_background, None);
    assert_eq!(style.transition_ms, engine.theme().metrics.interaction_ms);
}

#[test]
fn multiline_preserves_newlines_while_single_line_and_secret_keep_filtering() {
    let buffer = TextBuffer::multiline();
    buffer.set("原文\r\n译文\n第三行\u{7}".to_owned().into());
    assert!(buffer.is_multiline());
    assert_eq!(buffer.text().as_deref(), Some("原文\n译文\n第三行"));
    buffer.edit(InputEdit::Insert("\n第四行".to_owned().into()));
    assert_eq!(buffer.text().as_deref(), Some("原文\n译文\n第三行\n第四行"));
    for input in [TextBuffer::new(), TextBuffer::secret()] {
        input.set("a\r\nb".to_owned().into());
        assert!(!input.is_multiline());
        assert_eq!(input.with_value(str::to_owned), "ab");
    }
}

#[test]
fn word_selection_reverses_from_original_word_without_editing_value() {
    let buffer = TextBuffer::new();
    buffer.set("one cafe\u{301} three".to_owned().into());
    let revision = buffer.value_revision();
    buffer.edit(InputEdit::Preedit("pending".to_owned().into()));
    buffer.edit(InputEdit::SelectWord {
        at: 6,
        extend_from: None,
    });
    assert_eq!(buffer.selected_text().as_deref(), Some("cafe\u{301}"));
    assert!(!buffer.is_composing());
    let (head, anchor) = buffer.selection_display_bytes();
    let initial = Some((anchor, head));
    buffer.edit(InputEdit::SelectWord {
        at: 13,
        extend_from: initial,
    });
    assert_eq!(buffer.selected_text().as_deref(), Some("cafe\u{301} three"));
    buffer.edit(InputEdit::SelectWord {
        at: 1,
        extend_from: initial,
    });
    assert_eq!(buffer.selected_text().as_deref(), Some("one cafe\u{301}"));
    assert_eq!(buffer.value_revision(), revision);
}

#[test]
fn visual_line_selection_trims_separators_and_secret_selection_stays_masked() {
    let buffer = TextBuffer::multiline();
    buffer.set("first\nsecond".to_owned().into());
    buffer.edit(InputEdit::SelectLine {
        at: 2,
        visual_range: Some((0, 6)),
        extend_from: None,
    });
    assert_eq!(buffer.selected_text().as_deref(), Some("first"));
    buffer.edit(InputEdit::SelectLine {
        at: 8,
        visual_range: None,
        extend_from: None,
    });
    assert_eq!(buffer.selected_text().as_deref(), Some("second"));
    let secret = TextBuffer::secret();
    secret.set("a👩‍💻e\u{301}".to_owned().into());
    secret.edit(InputEdit::SelectRange(3, 6));
    assert_eq!(secret.selection_display_bytes(), (6, 3));
    assert!(secret.selected_text().is_none());
    secret.edit(InputEdit::SelectWord {
        at: 3,
        extend_from: None,
    });
    let presentation = secret.presentation();
    assert_eq!(presentation.text, "•••");
    assert_eq!((presentation.head, presentation.anchor), (9, 0));
}

#[test]
fn revealing_a_secret_paints_real_bytes_without_losing_protections() {
    let buffer = TextBuffer::secret();
    buffer.edit(InputEdit::Insert(Zeroizing::new("s3cret!".to_owned())));
    // Masked: bullets at bullet offsets, content never readable.
    assert_eq!(buffer.presentation().text, "•••••••");
    assert_eq!(buffer.presentation().head, 21); // seven bullets at 3 bytes each
    // Revealed through the host's explicit visibility: the real text at
    // true byte offsets, so caret navigation matches what is painted.
    let revealed = buffer.presentation_with(true);
    assert_eq!(revealed.text, "s3cret!");
    assert_eq!(revealed.head, 7);
    buffer.edit(InputEdit::Left(false));
    // Hiding again returns to bullet offsets without losing the content,
    // and the no-op set_secret keeps the layout revision stable.
    buffer.set_secret(true);
    assert_eq!(buffer.presentation().text, "•••••••");
    // The buffer keeps its input-method protections: the value never leaves
    // through text(), and selection_utf16 stays withheld while masked.
    assert!(buffer.text().is_none());
    assert_eq!(buffer.selection_utf16(), 0..0);
    // An ordinary buffer ignores the reveal flag entirely.
    let plain = TextBuffer::new();
    plain.set("ordinary".to_owned().into());
    assert_eq!(plain.presentation_with(true).text, "ordinary");
}
