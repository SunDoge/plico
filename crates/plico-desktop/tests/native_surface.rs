#![cfg(not(target_arch = "wasm32"))]
use plico::prelude::*;
use plico_desktop::{SurfaceSource, View};
use plico_renderer::platform::effect::{EffectCompletion, EffectPayload, EffectResult};
use plico_renderer::{
    EffectRequest, FrameSource, HostAction, Modifiers, PaintContext, PaintScene, Point,
    PointerButton, PointerEvent, PointerPhase, Scene, TextContext, UiEvent, Widget,
    WidgetEventResult, WidgetGeometry,
};

struct Demo {
    show: bool,
    overlay: bool,
}
impl View for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        let _ = column(cx)
            .key("container")
            .padding(10)
            .width(220.)
            .height(180.)
            .children(|cx| {
                if self.show {
                    let _ = column(cx)
                        .key("surface")
                        .width(150.)
                        .height(100.)
                        .children(|_cx| {});
                }
                if self.overlay {
                    let _ = column(cx)
                        .absolute(10., 10.)
                        .width(80.)
                        .height(80.)
                        .children(|cx| {
                            button(cx).text("Cover").width(80.).height(80.);
                        });
                }
            });
    }
}
#[derive(Default)]
struct Native {
    focused: bool,
    point: Option<Point>,
    mounts: usize,
    geometries: Vec<WidgetGeometry>,
    action: Option<HostAction>,
    clipboard: Option<(u64, Option<String>)>,
}
impl Widget for Native {
    fn mounted(&mut self) -> plico_renderer::WidgetChanges {
        self.mounts += 1;
        Default::default()
    }
    fn paint(&mut self, cx: &mut PaintContext<'_>) {
        cx.scene_mut().fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &kurbo::Rect::new(0., 0., 10., 10.),
        );
    }
    fn handle_event(&mut self, event: &UiEvent) -> WidgetEventResult {
        if let UiEvent::Pointer(p) = event {
            self.point = Some(p.position);
        }
        WidgetEventResult::HANDLED
    }
    fn layout_changed(&mut self, g: WidgetGeometry) {
        self.geometries.push(g);
    }
    fn focus_changed(&mut self, b: bool) -> plico_renderer::WidgetChanges {
        self.focused = b;
        Default::default()
    }
    fn ime_cursor_area(&self) -> Option<[f32; 4]> {
        Some([1., 2., 3., 4.])
    }
    fn accepts_text_input(&self) -> bool {
        true
    }
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.action.take()
    }
    fn deliver_clipboard_text(&mut self, id: u64, text: Option<String>) {
        self.clipboard = Some((id, text))
    }
}
fn pointer(x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase: PointerPhase::Down,
        position: Point { x, y },
        button: Some(PointerButton::Primary),
        buttons: 1,
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    })
}
#[test]
fn layout_local_input_ime_and_visibility_preserve_native_instance() {
    let mut s = SurfaceSource::new(
        Demo {
            show: true,
            overlay: false,
        },
        Native::default(),
        "surface",
    );
    let mut text = TextContext::new();
    s.set_device_scale(2.);
    let nodes = s.build_frame(&mut text, 300, 220);
    let geometry = s.widget.geometries.last().unwrap();
    assert_eq!(geometry.content_size, [150., 100.]);
    assert_eq!(geometry.device_scale, 2.);
    let x = geometry.local_to_window[4];
    let y = geometry.local_to_window[5];
    s.handle_event(pointer(x + 5., y + 7.));
    assert_eq!(s.widget.point, Some(Point { x: 5., y: 7. }));
    assert!(s.widget.focused);
    assert_eq!(
        s.ime_state().unwrap().cursor_area,
        [x + 1., y + 2., x + 3., y + 4.]
    );
    let mut scene = Scene::new();
    s.paint_scene(&mut scene, &nodes, &mut text, (300, 220), 2.);
    assert!(!scene.commands.is_empty());
    s.source.view.show = false;
    s.source.request_rebuild();
    s.build_frame(&mut text, 300, 220);
    assert!(!s.widget.focused);
    s.source.view.show = true;
    s.source.request_rebuild();
    s.build_frame(&mut text, 300, 220);
    assert_eq!(s.widget.mounts, 1);
}
#[test]
fn higher_plico_controls_win_hits_and_clipboard_actions_use_effects() {
    let mut s = SurfaceSource::new(
        Demo {
            show: true,
            overlay: true,
        },
        Native::default(),
        "surface",
    );
    let mut text = TextContext::new();
    s.build_frame(&mut text, 300, 220);
    s.handle_event(pointer(30., 30.));
    assert!(s.widget.point.is_none());
    s.widget.action = Some(HostAction::ReadClipboard { request_id: 17 });
    assert!(s.take_host_action().is_none());
    let EffectRequest { id, payload, .. } = s.take_effect().unwrap();
    assert_eq!(payload, EffectPayload::ClipboardRead);
    s.complete_effect(EffectCompletion {
        id,
        op: payload.op(),
        result: EffectResult::ClipboardText(Some("reply".into())),
    });
    assert_eq!(s.widget.clipboard, Some((17, Some("reply".into()))));
}
