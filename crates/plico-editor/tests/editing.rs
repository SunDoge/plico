use plico_editor::{CodeEditor, CodeEditorState};
use plico_renderer::{
    ImeEvent, KeyEvent, KeyLocation, KeyPhase, Modifiers, PaintContext, Point, PointerButton,
    PointerEvent, PointerPhase, TextContext, UiEvent, Widget, WidgetGeometry,
};

fn key(editor: &mut CodeEditor, name: &str, modifiers: Modifiers) {
    editor.handle_event(&UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: name.into(),
        key_without_modifiers: name.into(),
        code: String::new(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers,
        repeat: false,
        synthetic: false,
    }));
}
fn prepare(editor: &mut CodeEditor, text: &mut TextContext) {
    editor.layout_changed(WidgetGeometry {
        content_size: [600., 240.],
        ..Default::default()
    });
    editor.prepare_for_event(text);
}
#[test]
fn edits_restore_selection_and_redo_branch_is_discarded() {
    let s = CodeEditorState::new("hello 世界");
    assert!(s.select(6, 12));
    s.insert("Plico");
    assert_eq!(s.text(), "hello Plico");
    assert!(s.undo());
    assert_eq!(s.text(), "hello 世界");
    assert_eq!(s.selection(), [6, 12]);
    assert!(s.redo());
    assert_eq!(s.text(), "hello Plico");
    assert!(s.undo());
    s.insert("Rust");
    assert!(!s.redo());
    assert_eq!(s.text(), "hello Rust");
}
#[test]
fn utf8_boundaries_and_grapheme_deletion() {
    let s = CodeEditorState::new("中👨‍👩‍👧‍👦e\u{301}");
    assert!(!s.select(1, 0));
    assert!(!s.select(usize::MAX, 0));
    let end = s.text().len();
    s.select(end, end);
    let mut e = CodeEditor::new(s.clone());
    key(&mut e, "Backspace", Modifiers::empty());
    assert_eq!(s.text(), "中👨‍👩‍👧‍👦");
    key(&mut e, "Backspace", Modifiers::empty());
    assert_eq!(s.text(), "中");
}
#[test]
fn ime_updates_do_not_edit_document_and_commit_is_one_undo() {
    let s = CodeEditorState::new("abc");
    s.select(1, 2);
    let mut e = CodeEditor::new(s.clone());
    e.handle_event(&UiEvent::Ime(ImeEvent::Preedit {
        text: "ni".into(),
        cursor: Some((2, 2)),
    }));
    e.handle_event(&UiEvent::Ime(ImeEvent::Preedit {
        text: "你".into(),
        cursor: Some((3, 3)),
    }));
    assert_eq!(s.text(), "abc");
    assert_eq!(s.revision(), 0);
    e.handle_event(&UiEvent::Ime(ImeEvent::Commit("你".into())));
    assert_eq!(s.text(), "a你c");
    assert!(s.undo());
    assert_eq!(s.text(), "abc");
    assert!(!s.undo());
}
#[test]
fn lost_focus_discards_composition_and_clipboard_uses_selection() {
    let s = CodeEditorState::new("alpha\nbeta");
    s.select(0, 5);
    let mut e = CodeEditor::new(s.clone());
    e.focus_changed(true);
    let mut tcx = TextContext::new();
    prepare(&mut e, &mut tcx);
    assert_eq!(e.ime_state().unwrap().selection_utf16, 0..5);
    e.handle_event(&UiEvent::Ime(ImeEvent::Preedit {
        text: "临时".into(),
        cursor: None,
    }));
    e.focus_changed(false);
    assert!(e.ime_state().is_none());
    assert_eq!(s.text(), "alpha\nbeta");
    e.handle_event(&UiEvent::Paste("gamma".into()));
    assert_eq!(s.text(), "gamma\nbeta");
}
#[test]
fn click_hit_testing_uses_shaped_positions_after_edit_without_paint() {
    let s = CodeEditorState::new("abc世界");
    let mut e = CodeEditor::new(s.clone());
    let mut tcx = TextContext::new();
    prepare(&mut e, &mut tcx);
    e.handle_event(&UiEvent::TextInput("XYZ".into()));
    e.prepare_for_event(&mut tcx);
    e.handle_event(&UiEvent::Pointer(PointerEvent {
        phase: PointerPhase::Down,
        position: Point { x: 60., y: 15. },
        button: Some(PointerButton::Primary),
        buttons: 1,
        properties: Default::default(),
        modifiers: Modifiers::empty(),
    }));
    assert_eq!(s.selection(), [0, 0]);
    e.handle_event(&UiEvent::TextInput("!".into()));
    assert_eq!(s.text(), "!XYZabc世界");
}
#[test]
fn paint_highlight_and_scroll_only_visible_lines() {
    let s = CodeEditorState::new(
        &(0..1000)
            .map(|n| format!("let number_{n} = {n};\n"))
            .collect::<String>(),
    );
    let mut e = CodeEditor::new(s.clone());
    let mut tcx = TextContext::new();
    prepare(&mut e, &mut tcx);
    let mut paint = PaintContext::new(600., 240., 2., &mut tcx);
    e.paint(&mut paint);
    assert!(!paint.finish().commands.is_empty());
    let end = s.text().len();
    s.select(end, end);
    key(&mut e, "End", Modifiers::empty());
    e.prepare_for_event(&mut tcx);
    assert!(e.scroll_offset()[1] > 10000.);
    assert_eq!(s.text().lines().count(), 1000);
}

#[test]
fn deletion_undo_restores_caret_and_page_motion_keeps_visual_column() {
    let text = (0..40).map(|_| "abcdefghij\n").collect::<String>();
    let s = CodeEditorState::new(&text);
    s.select(5, 5);
    let mut e = CodeEditor::new(s.clone());
    let mut tcx = TextContext::new();
    prepare(&mut e, &mut tcx);
    key(&mut e, "Backspace", Modifiers::empty());
    assert_eq!(s.selection(), [4, 4]);
    assert!(s.undo());
    assert_eq!(s.selection(), [5, 5]);
    assert_eq!(s.text(), text);
    e.prepare_for_event(&mut tcx);
    key(&mut e, "PageDown", Modifiers::empty());
    e.prepare_for_event(&mut tcx);
    let expected_row = (240_f32 / 23.).floor() as usize;
    assert_eq!(
        s.selection(),
        [expected_row * 11 + 5, expected_row * 11 + 5]
    );
}

#[test]
fn composition_replacing_multiple_lines_shapes_without_changing_document() {
    let s = CodeEditorState::new("first\nsecond\nthird");
    s.select(2, 11);
    let mut e = CodeEditor::new(s.clone());
    e.focus_changed(true);
    e.handle_event(&UiEvent::Ime(ImeEvent::Preedit {
        text: "中文".into(),
        cursor: Some((0, 3)),
    }));
    let mut tcx = TextContext::new();
    prepare(&mut e, &mut tcx);
    let mut paint = PaintContext::new(600., 240., 1., &mut tcx);
    e.paint(&mut paint);
    assert!(!paint.finish().commands.is_empty());
    assert_eq!(s.text(), "first\nsecond\nthird");
    e.handle_event(&UiEvent::Ime(ImeEvent::Commit("中文".into())));
    assert_eq!(s.text(), "fi中文d\nthird");
    assert!(s.undo());
    assert_eq!(s.text(), "first\nsecond\nthird");
}
