use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, SemanticAction, SemanticRole, TextContext, UiEvent,
};
use std::time::Duration;
#[derive(Default)]
struct Demo {
    shows: usize,
    clicks: usize,
    disabled: bool,
    removed: bool,
    dialog: ModalState,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        column(cx)
            .padding(20)
            .gap(0)
            .align(Align::Start)
            .children(|cx| {
                if button(cx).text("Behind").width(180.).height(36.).clicked() {
                    self.clicks += 1;
                }
                if !self.removed {
                    tooltip(cx, "First helpful description").children(|cx| {
                        button(cx)
                            .text("First")
                            .width(180.)
                            .height(36.)
                            .disabled(self.disabled);
                    });
                }
                tooltip(cx, "Outer description").children(|cx| {
                    tooltip(cx, "Second helpful description").children(|cx| {
                        button(cx).text("Second");
                    });
                });
                modal(cx, &mut self.dialog)
                    .title("Dialog")
                    .motion(Motion::None)
                    .children(|cx| {
                        tooltip(cx, "Inside dialog description").children(|cx| {
                            button(cx).text("Inside");
                        });
                    });
            })
    }
}
fn frame(s: &mut Source<Demo>, tcx: &mut TextContext) -> Vec<plico_renderer::layout::PlacedNode> {
    let nodes = s.build_frame(tcx, 500, 350);
    assert!(s.last_error.is_none());
    nodes
}
fn bounds(s: &Source<Demo>, name: &str) -> [f32; 4] {
    s.adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, name, None)
        .unwrap()
        .bounds
}
fn pointer(phase: PointerPhase, r: [f32; 4]) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point {
            x: (r[0] + r[2]) as f64 / 2.,
            y: (r[1] + r[3]) as f64 / 2.,
        },
        button: Some(PointerButton::Primary),
        buttons: 1,
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    })
}
fn hover(s: &mut Source<Demo>, name: &str) {
    s.handle_event(pointer(PointerPhase::Move, bounds(s, name)));
}
fn visible(nodes: &[plico_renderer::layout::PlacedNode], text: &str) -> bool {
    nodes.iter().any(|n| n.paint.text.as_deref() == Some(text))
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
#[test]
fn injected_clock_delay_after_long_idle_and_waiting_frames_do_not_redeclare() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    clock.advance(Duration::from_secs(100)).unwrap();
    hover(&mut s, "First");
    assert!(!visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
    let shows = s.view.shows;
    clock.advance(Duration::from_millis(599)).unwrap();
    assert!(!visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
    assert_eq!(s.view.shows, shows);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    clock.advance(Duration::from_millis(1)).unwrap();
    assert!(visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
    assert_eq!(s.view.shows, shows);
    assert!(!s.has_anim());
}
#[test]
fn keyboard_focus_is_immediate_described_and_escape_suppresses_until_leave() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let initial = s.adapter.semantics(&s.engine, 0);
    let description = initial
        .node_by_role(SemanticRole::Tooltip, "First helpful description", None)
        .unwrap();
    assert_eq!(description.bounds, [0.; 4]);
    assert_eq!(
        initial
            .node_by_role(SemanticRole::Button, "First", None)
            .unwrap()
            .described_by,
        vec![description.id]
    );
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "First", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Focus { target }));
    assert!(visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
    let snapshot = s.adapter.semantics(&s.engine, 0);
    let button = snapshot
        .node_by_role(SemanticRole::Button, "First", None)
        .unwrap();
    let tip = snapshot
        .node_by_role(SemanticRole::Tooltip, "First helpful description", None)
        .unwrap();
    assert_eq!(button.described_by, vec![tip.id]);
    assert_eq!(snapshot.focus, Some(target));
    assert!(s.handle_event(key("Escape")).handled);
    assert!(!visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
    clock.advance(Duration::from_secs(2)).unwrap();
    assert!(!visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
    s.handle_event(key("Tab"));
    frame(&mut s, &mut tcx);
    s.handle_semantic_action(SemanticAction::Focus { target });
    assert!(visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
}
#[test]
fn toolbar_warm_window_and_innermost_description() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    hover(&mut s, "First");
    frame(&mut s, &mut tcx);
    clock.advance(Duration::from_millis(600)).unwrap();
    frame(&mut s, &mut tcx);
    hover(&mut s, "Second");
    let nodes = frame(&mut s, &mut tcx);
    assert!(visible(&nodes, "Second helpful description"));
    assert!(!visible(&nodes, "Outer description"));
    s.handle_event(pointer(PointerPhase::Leave, [0.; 4]));
    frame(&mut s, &mut tcx);
    clock.advance(Duration::from_millis(401)).unwrap();
    hover(&mut s, "First");
    assert!(!visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
}
#[test]
fn overlay_does_not_intercept_pointer_and_removed_disabled_anchors_do_not_show() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    hover(&mut s, "First");
    frame(&mut s, &mut tcx);
    clock.advance(Duration::from_millis(600)).unwrap();
    let nodes = frame(&mut s, &mut tcx);
    let rect = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("First helpful description"))
        .unwrap()
        .rect;
    let behind = bounds(&s, "Behind");
    assert!(rect[1] >= behind[1] && rect[3] <= behind[3]);
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        s.handle_event(pointer(phase, rect));
    }
    frame(&mut s, &mut tcx);
    assert_eq!(s.view.clicks, 1);
    s.view.disabled = true;
    s.request_rebuild();
    frame(&mut s, &mut tcx);
    hover(&mut s, "First");
    frame(&mut s, &mut tcx);
    clock.advance(Duration::from_secs(1)).unwrap();
    assert!(!visible(
        &frame(&mut s, &mut tcx),
        "First helpful description"
    ));
    s.view.removed = true;
    s.request_rebuild();
    frame(&mut s, &mut tcx);
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Tooltip, "First helpful description", None)
            .is_none()
    );
}
#[test]
fn tooltip_in_modal_escapes_clip_and_escape_hides_tip_before_dialog() {
    let clock = plico_core::VirtualClock::default();
    let mut view = Demo::default();
    view.dialog.open();
    let mut s = Source::with_clock(view, clock);
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx);
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Inside", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Focus { target });
    assert!(visible(
        &frame(&mut s, &mut tcx),
        "Inside dialog description"
    ));
    s.handle_event(key("Escape"));
    assert!(!visible(
        &frame(&mut s, &mut tcx),
        "Inside dialog description"
    ));
    assert!(s.view.dialog.is_open());
    s.handle_event(key("Escape"));
    frame(&mut s, &mut tcx);
    assert!(!s.view.dialog.is_open());
}

#[derive(Default)]
struct Editor {
    buffer: TextBuffer,
}
impl View for Editor {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        tooltip(cx, "Type a project name").children(|cx| {
            text_input(cx, "Name", &self.buffer);
        });
    }
}
#[test]
fn ime_gets_escape_before_tooltip_and_window_blur_hides_tip() {
    let mut s = Source::new(Editor::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 200);
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::TextInput, "Name", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Focus { target });
    assert!(visible(
        &s.build_frame(&mut tcx, 400, 200),
        "Type a project name"
    ));
    s.handle_event(UiEvent::Ime(plico_renderer::ImeEvent::Preedit {
        text: "合成".into(),
        cursor: None,
    }));
    s.handle_event(key("Escape"));
    assert!(!s.view.buffer.is_composing());
    assert!(visible(
        &s.build_frame(&mut tcx, 400, 200),
        "Type a project name"
    ));
    s.handle_event(UiEvent::Focus(false));
    assert!(!visible(
        &s.build_frame(&mut tcx, 400, 200),
        "Type a project name"
    ));
}
#[derive(Default)]
struct Shaded;
static EFFECT: ShaderAsset = ShaderAsset::new(
    "fn wabou_effect(uv: vec2<f32>) -> vec4<f32> {return vec4<f32>(0.0,1.0,0.0,1.0);}",
);
impl View for Shaded {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(20).align(Align::Start).children(|cx| {
            shader(cx, &EFFECT).width(240.).height(70.).animated(true);
            tooltip(cx, "Help on a shader screen").children(|cx| {
                button(cx).text("Help");
            });
        })
    }
}
#[test]
fn actual_gpu_tooltip_is_present_with_shader_and_scene_cache_removes_it_on_escape() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Shaded, clock.clone());
    let mut tcx = TextContext::new();
    let base = s.build_frame(&mut tcx, 400, 240);
    let mut base_scene = plico_renderer::Scene::default();
    s.paint_scene(&mut base_scene, &base, &mut tcx, (400, 240), 1.);
    let baseline =
        plico_shell::renderer::render_to_image(&base_scene, 400, 240, s.base_color()).unwrap();
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Help", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Focus { target });
    let nodes = s.build_frame(&mut tcx, 400, 240);
    let rect = nodes
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Help on a shader screen"))
        .unwrap()
        .rect;
    let (x, y) = ((rect[0] + 4.) as u32, ((rect[1] + rect[3]) / 2.) as u32);
    let mut scene = plico_renderer::Scene::default();
    s.paint_scene(&mut scene, &nodes, &mut tcx, (400, 240), 1.);
    let image = plico_shell::renderer::render_to_image(&scene, 400, 240, s.base_color()).unwrap();
    assert_eq!(image.get_pixel(x, y).0, [32, 32, 36, 255]);
    clock.advance(Duration::from_millis(100)).unwrap();
    s.build_frame(&mut tcx, 400, 240);
    assert_eq!(s.adapter.stats.tooltip_layout_runs, 0);
    assert_eq!(s.adapter.stats.layout_runs, 0);

    s.handle_event(key("Escape"));
    let nodes = s.build_frame(&mut tcx, 400, 240);
    s.paint_scene(&mut scene, &nodes, &mut tcx, (400, 240), 1.);
    let image = plico_shell::renderer::render_to_image(&scene, 400, 240, s.base_color()).unwrap();
    assert_eq!(image.get_pixel(x, y).0, baseline.get_pixel(x, y).0);
}

struct LongTip;
impl View for LongTip {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(8).align(Align::End).justify(Justify::End).children(|cx| {
            tooltip(cx,"A longer description that wraps and stays within the window when the focused control is near its edge.").children(|cx| {button(cx).text("Edge");});
        })
    }
}
#[test]
fn long_description_wraps_flips_and_repositions_on_resize() {
    let mut s = Source::new(LongTip);
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 220, 180);
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Edge", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Focus { target });
    for (w, h) in [(220, 180), (400, 240)] {
        let nodes = s.build_frame(&mut tcx, w, h);
        let tip = nodes
            .iter()
            .find(|p| {
                p.paint
                    .text
                    .as_deref()
                    .is_some_and(|t| t.starts_with("A longer description"))
            })
            .unwrap();
        assert!(
            tip.rect[0] >= 0.
                && tip.rect[1] >= 0.
                && tip.rect[2] <= w as f32
                && tip.rect[3] <= h as f32,
            "{:?}",
            tip.rect
        );
        let button = s
            .adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Button, "Edge", None)
            .unwrap()
            .bounds;
        assert!(tip.rect[3] < button[1]);
        assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(target));
    }
}
