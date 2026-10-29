use plico::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Scene, SemanticAction, SemanticRole, TextContext,
    UiEvent,
};
use plico_runtime::{
    Source,
    regions::{Access, BindState, OnChange, OnClickWith, OnSubmit, Regions, State, RegionUi},
};

fn paint(s: &mut Source<Regions>, text: &mut TextContext) {
    s.render_retained_frame(&mut Scene::new(), text, (700, 600), 1.)
        .unwrap();
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
}
fn target(s: &Source<Regions>, role: SemanticRole, label: &str) -> u64 {
    s.adapter
        .semantics(&s.engine, 0)
        .node_by_role(role, label, None)
        .unwrap()
        .id
}
fn root_runs(s: &Source<Regions>) -> usize {
    let id = s.engine.find(s.engine.root(), "regions-root").unwrap();
    s.view
        .region_runs()
        .into_iter()
        .find(|(e, _)| *e == id)
        .unwrap()
        .1
}
fn contains(s: &Source<Regions>, text: &str) -> bool {
    s.adapter
        .placed
        .iter()
        .any(|p| p.paint.text.as_deref() == Some(text))
}
fn enter() -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: "Enter".into(),
        code: "Enter".into(),
        key_without_modifiers: "Enter".into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
struct Form {
    source: Source<Regions>,
    enabled: State<bool>,
    volume: State<f64>,
    draft: State<TextBuffer>,
    submitted: State<usize>,
}
impl Form {
    fn new() -> Self {
        fn set_enabled((enabled,): (Access<'_, bool>,), value: bool) {
            enabled.set(value);
        }
        let mut saved = None;
        let app = Regions::new(|app| {
            let enabled = app.state(|| false);
            let volume = app.state(|| 25_f64);
            let draft = app.state(TextBuffer::new);
            let submitted = app.state(|| 0_usize);
            saved = Some((enabled, volume, draft, submitted));
            app.root(
                (enabled, volume, draft, submitted),
                |cx, (enabled, volume, draft, submitted)| {
                    column(cx).gap(8).children(|cx| {
                        cx.region_keyed(1, (enabled.handle(),), |cx, (enabled,)| {
                            let mut checked = *enabled.read();
                            checkbox(cx, "Enabled", &mut checked).bind(enabled.handle());
                            let mut checked = *enabled.read();
                            switch(cx, "Power", &mut checked)
                                .on_change((enabled.handle(),), set_enabled);
                        });
                        cx.region_keyed(2, (volume.handle(),), |cx, (volume,)| {
                            let mut value = *volume.read();
                            slider(cx, "Volume", &mut value, 0. ..=100.).bind(volume.handle());
                        });
                        cx.region_keyed(
                            3,
                            (draft.handle(), submitted.handle()),
                            |cx, (draft, submitted)| {
                                text_input(cx, "Draft", &draft.read())
                                    .placeholder("Type here")
                                    .bind(draft.handle())
                                    .on_submit((submitted.handle(),), |(submitted,)| {
                                        submitted.update(|v| *v += 1);
                                    });
                            },
                        );
                        cx.region_keyed(
                            4,
                            (enabled.handle(), volume.handle(), draft.handle()),
                            |cx, (enabled, volume, draft)| {
                                text!(
                                    cx,
                                    "Enabled {} Volume {:.0}",
                                    *enabled.read(),
                                    *volume.read()
                                )
                                .color(if *enabled.read() {
                                    Color::Accent
                                } else {
                                    Color::Muted
                                });
                                text!(cx, "Draft: {}", draft.read().text().unwrap_or_default());
                            },
                        );
                    });
                },
            );
        });
        let (enabled, volume, draft, submitted) = saved.unwrap();
        let mut source = Source::new(app);
        source.set_reduced_motion(true);
        Self {
            source,
            enabled,
            volume,
            draft,
            submitted,
        }
    }
    fn focus(&mut self, text: &mut TextContext) {
        let id = target(&self.source, SemanticRole::TextInput, "Draft");
        assert!(
            self.source
                .handle_semantic_action(SemanticAction::Focus { target: id })
        );
        paint(&mut self.source, text);
    }
}
#[test]
fn toggle_bind_and_change_payload_write_back_without_replaying_root() {
    let mut form = Form::new();
    let mut text = TextContext::new();
    paint(&mut form.source, &mut text);
    let calls = root_runs(&form.source);
    let id = target(&form.source, SemanticRole::CheckBox, "Enabled");
    assert!(
        form.source
            .handle_semantic_action(SemanticAction::Click { target: id })
    );
    paint(&mut form.source, &mut text);
    assert!(*form.source.view.access(form.enabled).read());
    assert!(contains(&form.source, "Enabled true Volume 25"));
    assert_eq!(root_runs(&form.source), calls);
    let id = target(&form.source, SemanticRole::Switch, "Power");
    form.source
        .handle_semantic_action(SemanticAction::Click { target: id });
    paint(&mut form.source, &mut text);
    assert!(!*form.source.view.access(form.enabled).read());
    assert_eq!(root_runs(&form.source), calls);
}
#[test]
fn slider_semantic_value_is_clamped_and_updates_dependent_text() {
    let mut form = Form::new();
    let mut text = TextContext::new();
    paint(&mut form.source, &mut text);
    let calls = root_runs(&form.source);
    let id = target(&form.source, SemanticRole::Slider, "Volume");
    assert!(
        form.source
            .handle_semantic_action(SemanticAction::SetNumericValue {
                target: id,
                value: 75.
            })
    );
    paint(&mut form.source, &mut text);
    assert_eq!(*form.source.view.access(form.volume).read(), 75.);
    assert!(contains(&form.source, "Enabled false Volume 75"));
    assert_eq!(root_runs(&form.source), calls);
    form.source
        .handle_semantic_action(SemanticAction::SetNumericValue {
            target: id,
            value: 200.,
        });
    paint(&mut form.source, &mut text);
    assert_eq!(*form.source.view.access(form.volume).read(), 100.);
}
#[test]
fn typing_and_submission_route_to_input_region_and_preserve_focus() {
    let mut form = Form::new();
    let mut text = TextContext::new();
    paint(&mut form.source, &mut text);
    form.focus(&mut text);
    let calls = root_runs(&form.source);
    assert!(
        form.source
            .handle_event(UiEvent::TextInput("hello 世界".into()))
            .handled
    );
    paint(&mut form.source, &mut text);
    assert_eq!(
        form.source.view.access(form.draft).read().text().as_deref(),
        Some("hello 世界")
    );
    assert!(contains(&form.source, "Draft: hello 世界"));
    assert_eq!(root_runs(&form.source), calls);
    assert!(form.source.handle_event(enter()).handled);
    paint(&mut form.source, &mut text);
    assert_eq!(*form.source.view.access(form.submitted).read(), 1);
    paint(&mut form.source, &mut text);
    assert_eq!(*form.source.view.access(form.submitted).read(), 1);
    assert!(form.source.ime_state().is_some());
}
#[test]
fn ime_commit_updates_preview_and_retains_editor_binding() {
    let mut form = Form::new();
    let mut text = TextContext::new();
    paint(&mut form.source, &mut text);
    form.focus(&mut text);
    form.source
        .handle_event(UiEvent::Ime(plico_renderer::ImeEvent::Preedit {
            text: "候補".into(),
            cursor: None,
        }));
    paint(&mut form.source, &mut text);
    assert_eq!(
        form.source.view.access(form.draft).read().text().as_deref(),
        Some("")
    );
    let calls = root_runs(&form.source);
    form.source
        .handle_event(UiEvent::Ime(plico_renderer::ImeEvent::Commit(
            "確認".into(),
        )));
    paint(&mut form.source, &mut text);
    assert!(contains(&form.source, "Draft: 確認"));
    assert_eq!(root_runs(&form.source), calls);
}
#[test]
fn payload_row_removal_and_reordering_preserve_surviving_widget_ids() {
    fn remove((rows,): (Access<'_, Vec<u64>>,), id: u64) {
        rows.update(|rows| rows.retain(|v| *v != id));
    }
    let mut saved = None;
    let app = Regions::new(|app| {
        let rows = app.state(|| vec![10_u64, 20, 30]);
        saved = Some(rows);
        app.root((rows,), |cx, (rows,)| {
            cx.region_keyed(1, (rows.handle(),), |cx, (rows,)| {
                for &id in rows.read().iter() {
                    row(cx).key(id).children(|cx| {
                        text!(cx, "Row {id}").key("label");
                        button(cx).text(format!("Remove {id}")).on_click_with(
                            (rows.handle(),),
                            id,
                            remove,
                        );
                    });
                }
            });
        });
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    paint(&mut source, &mut text);
    let calls = root_runs(&source);
    let survivor = target(&source, SemanticRole::Button, "Remove 30");
    let removed = target(&source, SemanticRole::Button, "Remove 20");
    source.handle_semantic_action(SemanticAction::Click { target: removed });
    paint(&mut source, &mut text);
    assert_eq!(&*source.view.access(saved.unwrap()).read(), &[10, 30]);
    assert!(!source.handle_semantic_action(SemanticAction::Click { target: removed }));
    assert_eq!(target(&source, SemanticRole::Button, "Remove 30"), survivor);
    source.view.access(saved.unwrap()).update(|v| v.reverse());
    paint(&mut source, &mut text);
    assert_eq!(target(&source, SemanticRole::Button, "Remove 30"), survivor);
    assert_eq!(root_runs(&source), calls);
}

#[test]
fn secret_buffer_binding_keeps_password_out_of_presentation_and_ime() {
    let app = Regions::new(|app| {
        let secret = app.state(TextBuffer::secret);
        app.root((secret,), |cx, (secret,)| {
            cx.region((secret.handle(),), |cx, (secret,)| {
                text_input(cx, "Password", &secret.read()).bind(secret.handle());
            });
        });
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    paint(&mut source, &mut text);
    let id = target(&source, SemanticRole::TextInput, "Password");
    source.handle_semantic_action(SemanticAction::Focus { target: id });
    paint(&mut source, &mut text);
    source.handle_event(UiEvent::TextInput("not-for-presentation".into()));
    paint(&mut source, &mut text);
    assert_eq!(source.ime_state().unwrap().surrounding_text, "");
    assert!(
        !serde_json::to_string(&source.engine.snapshot())
            .unwrap()
            .contains("not-for-presentation")
    );
}
#[test]
fn focused_input_unmount_clears_ime_and_remount_preserves_buffer() {
    let mut saved = None;
    let app = Regions::new(|app| {
        let visible = app.state(|| true);
        let draft = app.state(TextBuffer::new);
        saved = Some((visible, draft));
        app.root((visible, draft), |cx, (visible, draft)| {
            cx.region(
                (visible.handle(), draft.handle()),
                |cx, (visible, draft)| {
                    if *visible.read() {
                        text_input(cx, "Optional input", &draft.read()).bind(draft.handle());
                    }
                },
            );
        });
    });
    let (visible, draft) = saved.unwrap();
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    paint(&mut source, &mut text);
    let old = target(&source, SemanticRole::TextInput, "Optional input");
    source.handle_semantic_action(SemanticAction::Focus { target: old });
    paint(&mut source, &mut text);
    source.handle_event(UiEvent::TextInput("retained".into()));
    paint(&mut source, &mut text);
    source.view.access(visible).set(false);
    paint(&mut source, &mut text);
    assert!(source.ime_state().is_none());
    assert!(!source.handle_semantic_action(SemanticAction::Focus { target: old }));
    source.view.access(visible).set(true);
    paint(&mut source, &mut text);
    assert_ne!(
        target(&source, SemanticRole::TextInput, "Optional input"),
        old
    );
    assert_eq!(
        source.view.access(draft).read().text().as_deref(),
        Some("retained")
    );
}
