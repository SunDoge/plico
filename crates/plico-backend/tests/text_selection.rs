use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_renderer::{
    ClipboardRequest, FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point,
    PointerButton, PointerEvent, PointerPhase, PointerProperties, TextContext, UiEvent,
};
#[derive(Default)]
struct Document {
    suffix: bool,
    replace: bool,
    hidden: bool,
    input: TextBuffer,
}
impl View for Document {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width(300.).gap(12).children(|cx| {
            text_input(cx, "Search", &self.input);
            if !self.hidden {
                let _ = column(cx).selectable(true).gap(10).children(|cx| {
                    text(
                        cx,
                        if self.replace {
                            "Replaced"
                        } else {
                            "First 中文 paragraph"
                        },
                    )
                    .width_percent(100.);
                    text(
                        cx,
                        if self.suffix {
                            "Second paragraph appended"
                        } else {
                            "Second paragraph"
                        },
                    )
                    .width_percent(100.);
                });
            }
            let _ = column(cx).selectable(true).children(|cx| {
                text(cx, "Separate document");
            });
            text(cx, "Not selectable");
        });
    }
}
fn pointer(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y },
        button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
            .then_some(PointerButton::Primary),
        buttons: 1,
        modifiers: Modifiers::empty(),
        properties: PointerProperties::default(),
    })
}
fn key(value: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        key_without_modifiers: value.into(),
        code: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: if value == "Escape" {
            Modifiers::empty()
        } else {
            Modifiers::CONTROL
        },
        repeat: false,
        synthetic: false,
    })
}
fn origin(s: &Source<Document>, prefix: &str) -> [f64; 2] {
    let p = s
        .adapter
        .placed
        .iter()
        .find(|p| {
            p.paint
                .text
                .as_deref()
                .is_some_and(|t| t.starts_with(prefix))
        })
        .unwrap();
    [
        f64::from(p.content_origin[0]) + 1.,
        f64::from(p.content_origin[1]) + 5.,
    ]
}
fn frame(s: &mut Source<Document>, tcx: &mut TextContext) {
    s.build_frame(tcx, 500, 400);
    assert!(s.last_error.is_none());
}
#[test]
fn cross_paragraph_drag_reverse_copy_select_all_and_clear() {
    let mut s = Source::new(Document::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let a = origin(&s, "First");
    let b = origin(&s, "Second");
    s.handle_event(pointer(PointerPhase::Down, a[0], a[1]));
    s.handle_event(pointer(PointerPhase::Move, b[0] + 45., b[1]));
    s.handle_event(pointer(PointerPhase::Up, b[0] + 45., b[1]));
    frame(&mut s, &mut tcx);
    let selected = s.adapter.selected_text().unwrap();
    assert!(selected.starts_with("First 中文 paragraph\n\n"));
    assert!(
        s.adapter
            .placed
            .iter()
            .filter(|p| !p.paint.selection_rects.is_empty())
            .count()
            >= 2
    );
    assert_eq!(
        s.handle_event(key("c")).clipboard,
        Some(ClipboardRequest::Write(selected.clone()))
    );
    s.handle_event(pointer(PointerPhase::Down, b[0] + 45., b[1]));
    s.handle_event(pointer(PointerPhase::Move, a[0], a[1]));
    s.handle_event(pointer(PointerPhase::Up, a[0], a[1]));
    assert_eq!(s.adapter.selected_text().unwrap(), selected);
    s.handle_event(key("a"));
    frame(&mut s, &mut tcx);
    assert_eq!(
        s.adapter.selected_text().unwrap(),
        "First 中文 paragraph\n\nSecond paragraph"
    );
    assert!(!s.adapter.selected_text().unwrap().contains("Separate"));
    s.handle_event(key("Escape"));
    frame(&mut s, &mut tcx);
    assert!(s.adapter.selected_text().is_none());
    assert!(
        s.adapter
            .placed
            .iter()
            .all(|p| p.paint.selection_rects.is_empty())
    );
}
#[test]
fn append_preserves_selection_replacement_and_unmount_clear_it() {
    let mut s = Source::new(Document::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let a = origin(&s, "First");
    s.handle_event(pointer(PointerPhase::Down, a[0], a[1]));
    s.handle_event(pointer(PointerPhase::Up, a[0], a[1]));
    s.handle_event(key("a"));
    s.view.suffix = true;
    frame(&mut s, &mut tcx);
    assert_eq!(
        s.adapter.selected_text().unwrap(),
        "First 中文 paragraph\n\nSecond paragraph"
    );
    s.view.replace = true;
    frame(&mut s, &mut tcx);
    assert!(s.adapter.selected_text().is_none());
    let a = origin(&s, "Replaced");
    s.handle_event(pointer(PointerPhase::Down, a[0], a[1]));
    s.handle_event(key("a"));
    s.view.hidden = true;
    frame(&mut s, &mut tcx);
    assert!(s.adapter.selected_text().is_none());
}
#[test]
fn clicking_blank_space_clears_highlights_without_node_changes() {
    let mut s = Source::new(Document::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let a = origin(&s, "First");
    s.handle_event(pointer(PointerPhase::Down, a[0], a[1]));
    s.handle_event(pointer(PointerPhase::Up, a[0], a[1]));
    s.handle_event(key("a"));
    frame(&mut s, &mut tcx);
    assert!(
        s.adapter
            .placed
            .iter()
            .any(|p| !p.paint.selection_rects.is_empty())
    );

    let response = s.handle_event(pointer(PointerPhase::Down, 450., 350.));
    assert!(response.request_redraw);
    frame(&mut s, &mut tcx);
    assert!(s.adapter.selected_text().is_none());
    assert!(
        s.adapter
            .placed
            .iter()
            .all(|p| p.paint.selection_rects.is_empty())
    );
}
#[test]
fn ordinary_labels_do_not_select_and_input_keeps_its_own_shortcuts() {
    let mut s = Source::new(Document::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let a = origin(&s, "Not selectable");
    s.handle_event(pointer(PointerPhase::Down, a[0], a[1]));
    s.handle_event(key("a"));
    assert!(s.adapter.selected_text().is_none());
    let input = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(plico_renderer::SemanticRole::TextInput, "Search", None)
        .unwrap()
        .id;
    s.handle_semantic_action(plico_renderer::SemanticAction::Focus { target: input });
    s.handle_event(UiEvent::TextInput("editable".into()));
    s.handle_event(key("a"));
    assert_eq!(
        s.handle_event(key("c")).clipboard,
        Some(ClipboardRequest::Write("editable".into()))
    );
}
