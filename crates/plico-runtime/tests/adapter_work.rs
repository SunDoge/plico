use plico::prelude::*;
use plico_renderer::{
    FrameSource, ImeEvent, Scene, SemanticAction, SemanticRole, TextContext, UiEvent,
};
use plico_runtime::{HostView, Source};
use std::sync::Arc;
struct Form {
    buffers: Vec<TextBuffer>,
    offset: f32,
    width: f32,
    placeholder: &'static str,
    selectable: bool,
    dark: bool,
    checked: bool,
    font: u16,
}
impl Form {
    fn new(count: usize) -> Self {
        Self {
            buffers: (0..count).map(|_| TextBuffer::new()).collect(),
            offset: 10.,
            width: 250.,
            placeholder: "Hint",
            selectable: false,
            dark: false,
            checked: false,
            font: 14,
        }
    }
}
impl HostView for Form {
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }
    #[component]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        column(cx).gap(8).children(|cx| {
            column(cx).height(self.offset).children(|_| {});
            column(cx).selectable(self.selectable).children(|cx| {
                text(cx, "Selectable label");
            });
            checkbox(cx, "Unrelated toggle", &mut self.checked);
            for (index, buffer) in self.buffers.iter().enumerate() {
                cx.keyed(index as u64, |cx| {
                    text_input(cx, format!("Field {index}"), buffer)
                        .width(self.width)
                        .height(36.)
                        .font_size(self.font)
                        .placeholder(self.placeholder);
                });
            }
        });
    }
}
fn frame(s: &mut Source<Form>, text: &mut TextContext) {
    s.render_retained_frame(&mut Scene::new(), text, (600, 2200), 1.)
        .unwrap();
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
}
fn input<'a>(s: &'a Source<Form>, label: &str) -> &'a plico_renderer::layout::PlacedNode {
    let bounds = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::TextInput, label, None)
        .unwrap()
        .bounds;
    s.adapter
        .placed
        .iter()
        .find(|n| n.rect == bounds && n.paint.widget.is_some())
        .unwrap()
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
fn moving_unchanged_inputs_reuses_local_fragments_and_focus_geometry() {
    let mut s = Source::new(Form::new(20));
    s.set_reduced_motion(true);
    let mut text = TextContext::new();
    frame(&mut s, &mut text);
    assert_eq!(s.adapter.stats.input_rebuilds, 20);
    assert_eq!(s.adapter.stats.selection_visits, 0);
    assert_eq!(s.adapter.stats.slider_visits, 0);
    assert_eq!(s.adapter.stats.shader_visits, 0);
    focus(&mut s, "Field 0");
    frame(&mut s, &mut text);
    let caret = s.ime_state().unwrap().cursor_area;
    let fragment = input(&s, "Field 0").paint.widget.clone().unwrap();
    s.view.offset += 40.;
    s.request_rebuild();
    frame(&mut s, &mut text);
    assert!(s.adapter.stats.layout_runs > 0);
    assert_eq!(s.adapter.stats.input_rebuilds, 0);
    assert_eq!(s.adapter.stats.input_reuses, 20);
    assert!(Arc::ptr_eq(
        &fragment,
        input(&s, "Field 0").paint.widget.as_ref().unwrap()
    ));
    let next = s.ime_state().unwrap().cursor_area;
    assert!((next[1] - caret[1] - 40.).abs() < 1.);
    assert!((next[3] - caret[3] - 40.).abs() < 1.);
    assert!(s.handle_event(UiEvent::TextInput("中文".into())).handled);
    frame(&mut s, &mut text);
    assert_eq!(
        s.adapter.stats.input_rebuilds, 1,
        "editing one buffer must not repaint the other 19"
    );
    assert_eq!(
        s.adapter.stats.input_reuses, 0,
        "fixed-size editing does not even visit unrelated inputs"
    );
    assert_eq!(s.view.buffers[0].text().as_deref(), Some("中文"));
    assert!(!Arc::ptr_eq(
        &fragment,
        input(&s, "Field 0").paint.widget.as_ref().unwrap()
    ));
    let fragment = input(&s, "Field 0").paint.widget.clone().unwrap();
    s.handle_event(UiEvent::Ime(ImeEvent::Preedit {
        text: "候補".into(),
        cursor: None,
    }));
    frame(&mut s, &mut text);
    assert!(s.adapter.stats.input_rebuilds >= 1);
    assert!(!Arc::ptr_eq(
        &fragment,
        input(&s, "Field 0").paint.widget.as_ref().unwrap()
    ));
}
#[test]
fn input_fragment_cache_invalidates_for_context_style_size_and_buffer_rebinding() {
    let mut s = Source::new(Form::new(2));
    s.set_reduced_motion(true);
    let mut text = TextContext::new();
    frame(&mut s, &mut text);
    let first = input(&s, "Field 0").paint.widget.clone().unwrap();
    s.view.buffers[0] = TextBuffer::new();
    s.view.offset += 1.;
    s.request_rebuild();
    frame(&mut s, &mut text);
    assert_eq!(
        s.adapter.stats.input_rebuilds, 1,
        "different empty buffers still have different caret/revision identity"
    );
    assert!(!Arc::ptr_eq(
        &first,
        input(&s, "Field 0").paint.widget.as_ref().unwrap()
    ));
    for change in 0..7 {
        match change {
            0 => s.view.placeholder = "New hint",
            1 => s.view.width = 210.,
            2 => s.view.dark = true,
            3 => text.clear_caches(),
            4 => text = TextContext::new(),
            5 => s.set_font_family("monospace"),
            _ => s.view.font = 20,
        }
        s.view.offset += 1.;
        s.request_rebuild();
        frame(&mut s, &mut text);
        assert_eq!(
            s.adapter.stats.input_rebuilds, 2,
            "cache invalidation {change}"
        );
    }
}
#[test]
fn enabling_and_removing_last_selectable_owner_updates_selection_index() {
    let mut s = Source::new(Form::new(1));
    let mut text = TextContext::new();
    frame(&mut s, &mut text);
    assert_eq!(s.adapter.stats.selection_visits, 0);
    s.view.selectable = true;
    s.request_rebuild();
    frame(&mut s, &mut text);
    assert!(s.adapter.stats.selection_visits > 0);
    s.view.selectable = false;
    s.request_rebuild();
    frame(&mut s, &mut text);
    // The previous selectable paragraph is invalidated once; later paint/layout
    // work must no longer rebuild an empty selection model.
    s.view.offset += 1.;
    s.request_rebuild();
    frame(&mut s, &mut text);
    assert_eq!(s.adapter.stats.selection_visits, 0);
    assert!(s.adapter.selected_text().is_none());
}

#[test]
fn shader_registry_tracks_mount_and_unmount_without_gpu_rasterization() {
    static EFFECT: ShaderAsset =
        ShaderAsset::new("fn wabou_effect(_uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }");
    struct Demo {
        present: bool,
    }
    impl HostView for Demo {
        #[component]
        fn show(&mut self, cx: &mut UiContext<'_>) {
            column(cx).children(|cx| {
                for index in 0..100 {
                    cx.keyed(index as u64, |cx| {
                        text(cx, "Unrelated text");
                    });
                }
                if self.present {
                    shader(cx, &EFFECT).width(32.).height(32.);
                }
            });
        }
    }
    let mut source = Source::new(Demo { present: true });
    let mut text = TextContext::new();
    source
        .render_retained_frame(&mut Scene::new(), &mut text, (600, 2200), 1.)
        .unwrap();
    assert!(source.last_error.is_none());
    assert_eq!(source.adapter.stats.shader_visits, 1);
    source.view.present = false;
    source.request_rebuild();
    source
        .render_retained_frame(&mut Scene::new(), &mut text, (600, 2200), 1.)
        .unwrap();
    assert_eq!(source.adapter.stats.shader_visits, 0);
    assert!(source.adapter.shader_diagnostics().next().is_none());
}

#[test]
fn unrelated_leaf_paint_does_not_rebuild_selectable_paragraphs() {
    let mut s = Source::new(Form::new(1));
    s.view.selectable = true;
    s.set_reduced_motion(true);
    let mut text = TextContext::new();
    frame(&mut s, &mut text);
    assert!(s.adapter.stats.selection_visits > 0);
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::CheckBox, "Unrelated toggle", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target }));
    frame(&mut s, &mut text);
    assert!(s.view.checked);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(s.adapter.stats.selection_visits, 0);
}

#[test]
fn scroll_feedback_uses_only_scroll_ports_and_clears_after_disabling_scroll() {
    struct Demo {
        checked: bool,
        scroll: bool,
    }
    impl HostView for Demo {
        #[component]
        fn show(&mut self, cx: &mut UiContext<'_>) {
            column(cx).children(|cx| {
                checkbox(cx, "Toggle", &mut self.checked);
                column(cx)
                    .height(120.)
                    .scroll_y(self.scroll)
                    .children(|cx| {
                        for index in 0..200_u64 {
                            cx.keyed(index, |cx| {
                                text!(cx, "Row {index}");
                            });
                        }
                    });
            });
        }
    }
    let mut s = Source::new(Demo {
        checked: false,
        scroll: true,
    });
    s.set_reduced_motion(true);
    let mut text = TextContext::new();
    s.render_retained_frame(&mut Scene::new(), &mut text, (600, 600), 1.)
        .unwrap();
    assert_eq!(s.adapter.stats.scroll_visits, 1);
    let before = s.batch_stats().scroll_viewport_visits;
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::CheckBox, "Toggle", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target }));
    s.render_retained_frame(&mut Scene::new(), &mut text, (600, 600), 1.)
        .unwrap();
    assert!(s.last_error.is_none());
    assert_eq!(s.batch_stats().scroll_viewport_visits - before, 1);
    assert_eq!(s.adapter.stats.scroll_visits, 1);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    s.view.scroll = false;
    s.request_rebuild();
    s.render_retained_frame(&mut Scene::new(), &mut text, (600, 600), 1.)
        .unwrap();
    assert_eq!(s.adapter.stats.scroll_visits, 0);
    let before = s.batch_stats().scroll_viewport_visits;
    s.request_rebuild();
    s.render_retained_frame(&mut Scene::new(), &mut text, (600, 600), 1.)
        .unwrap();
    assert_eq!(s.batch_stats().scroll_viewport_visits, before);
}

#[test]
fn geometry_alignment_only_visits_controls_and_reuses_metrics_without_origin_drift() {
    let mut s = Source::new(Form::new(20));
    s.set_reduced_motion(true);
    let mut text = TextContext::new();
    frame(&mut s, &mut text);
    assert_eq!(s.adapter.stats.control_metric_builds, 1);
    let origin = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Unrelated toggle"))
        .unwrap()
        .content_origin;
    for _ in 0..3 {
        s.view.offset += 40.;
        s.request_rebuild();
        frame(&mut s, &mut text);
        assert_eq!(s.adapter.stats.control_align_checks, 1);
        assert_eq!(s.adapter.stats.input_align_checks, 20);
        assert_eq!(s.adapter.stats.control_metric_builds, 0);
        assert_eq!(s.adapter.stats.control_metric_reuses, 1);
    }
    let next = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Unrelated toggle"))
        .unwrap()
        .content_origin;
    assert_eq!(next[0], origin[0]);
    assert!((next[1] - origin[1] - 120.).abs() < 0.01);
    s.view.dark = true;
    s.request_rebuild();
    frame(&mut s, &mut text);
    assert_eq!(s.adapter.stats.control_metric_builds, 1);
    text.clear_caches();
    s.view.offset += 1.;
    s.request_rebuild();
    frame(&mut s, &mut text);
    assert_eq!(s.adapter.stats.control_metric_builds, 1);
}

#[derive(Clone)]
struct Controls {
    visible: bool,
    label: &'static str,
    width: f32,
    font: u16,
    checked: bool,
}
impl HostView for Controls {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).children(|cx| {
            if self.visible {
                checkbox(cx, self.label, &mut self.checked)
                    .width(self.width)
                    .font_size(self.font);
            }
        });
    }
}
#[test]
fn control_metric_invalidation_matches_fresh_layout_and_cleans_unmounted_membership() {
    let mut s = Source::new(Controls {
        visible: true,
        label: "A label that wraps when the control becomes narrow",
        width: 300.,
        font: 14,
        checked: false,
    });
    s.set_reduced_motion(true);
    let mut text = TextContext::new();
    let draw = |s: &mut Source<Controls>, text: &mut TextContext| {
        s.render_retained_frame(&mut Scene::new(), text, (600, 600), 1.)
            .unwrap();
        assert!(s.last_error.is_none(), "{:?}", s.last_error);
    };
    draw(&mut s, &mut text);
    for change in 0..3 {
        match change {
            0 => s.view.width = 120.,
            1 => s.view.font = 24,
            _ => s.view.label = "New label",
        }
        s.request_rebuild();
        draw(&mut s, &mut text);
        assert_eq!(s.adapter.stats.control_metric_builds, 1);
        let mut fresh = Source::new(s.view.clone());
        fresh.set_reduced_motion(true);
        draw(&mut fresh, &mut text);
        let actual = s
            .adapter
            .placed
            .iter()
            .find(|n| n.paint.text.as_deref() == Some(s.view.label))
            .unwrap();
        let expected = fresh
            .adapter
            .placed
            .iter()
            .find(|n| n.paint.text.as_deref() == Some(s.view.label))
            .unwrap();
        assert_eq!(actual.rect, expected.rect);
        assert_eq!(actual.content_origin, expected.content_origin);
        assert_eq!(actual.content_size, expected.content_size);
    }
    s.view.visible = false;
    s.request_rebuild();
    draw(&mut s, &mut text);
    assert_eq!(s.adapter.stats.control_align_checks, 0);
    assert_eq!(s.adapter.stats.control_metric_reuses, 0);
    s.view.visible = true;
    s.request_rebuild();
    draw(&mut s, &mut text);
    assert_eq!(s.adapter.stats.control_align_checks, 1);
    assert_eq!(s.adapter.stats.control_metric_builds, 1);
}
