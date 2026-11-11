//! Real host geometry and retained-frame scheduling, without a GPU/window.
use plico::{
    UiContext, VirtualClock,
    widgets::{Toast, ToastHost, toast_viewport},
};
use plico_renderer::{FrameSource, Point, PointerEvent, PointerPhase, Scene, TextContext, UiEvent};
use plico_runtime::{HostView, Source};
use std::time::Duration;

#[derive(Default)]
struct View {
    toasts: ToastHost,
}
impl HostView for View {
    fn show(&mut self, cx: &mut UiContext) {
        toast_viewport(cx, &mut self.toasts);
    }
}
#[test]
fn hovering_noninteractive_toast_content_pauses_without_spinning_and_exit_keeps_waking() {
    let clock = VirtualClock::default();
    let mut view = View::default();
    view.toasts.toast(Toast::message("Real hover"));
    let mut source = Source::with_clock(view, clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    clock.advance(Duration::from_millis(140)).unwrap();
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    let card = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.background.is_some())
        .unwrap()
        .rect;
    let point = Point {
        x: f64::from(card[0] + 20.),
        y: f64::from(card[1] + 15.),
    };
    // This is the card body, not a button. Normal interactive hit-testing
    // declines it; the explicitly observed group still gets hover feedback.
    assert!(source.hit_test(point).is_none());
    let event = |phase| {
        UiEvent::Pointer(PointerEvent {
            phase,
            position: point,
            button: None,
            buttons: 0,
            modifiers: Default::default(),
            properties: Default::default(),
        })
    };
    assert!(
        source
            .handle_event(event(PointerPhase::Move))
            .request_redraw
    );
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert!(
        !source.has_anim(),
        "held, settled toast should let the host idle"
    );
    assert_eq!(
        source.animation_deadline(),
        None,
        "hover cancels expiry wake"
    );
    clock.advance(Duration::from_secs(10)).unwrap();
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert_eq!(source.view().toasts.len(), 1);
    assert!(
        source
            .handle_event(event(PointerPhase::Leave))
            .request_redraw
    );
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert_eq!(
        source.view().toasts.len(),
        1,
        "idle hover time must not be charged on release"
    );
    clock.advance(Duration::from_millis(3860)).unwrap();
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert!(source.view().toasts.is_empty());
    assert!(
        source.has_anim(),
        "exit must remain driven even though logical len is zero"
    );
    clock.advance(Duration::from_millis(140)).unwrap();
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert!(!source.has_anim());
}

#[test]
fn toast_portal_escapes_a_small_clipped_calling_container() {
    struct Nested {
        toasts: ToastHost,
    }
    impl HostView for Nested {
        fn show(&mut self, cx: &mut UiContext) {
            plico::widgets::column(cx)
                .width(100.)
                .height(20.)
                .clip(true)
                .children(|cx| {
                    toast_viewport(cx, &mut self.toasts);
                });
        }
    }
    let clock = VirtualClock::default();
    let mut host = ToastHost::default();
    host.toast(Toast::message("Portal").sticky());
    let mut source = Source::with_clock(Nested { toasts: host }, clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    clock.advance(Duration::from_millis(140)).unwrap();
    source
        .try_render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    let card = source
        .placed_nodes()
        .iter()
        .find(|node| node.paint.background.is_some())
        .unwrap()
        .rect;
    assert!(
        card[1] > 200.,
        "card anchors to the window, not the 20px calling container"
    );
    assert!(
        source
            .hit_test(Point {
                x: f64::from(card[2] - 25.),
                y: f64::from(card[1] + 21.)
            })
            .is_some(),
        "the close button must escape the caller's clip too"
    );
}

#[test]
fn settled_toast_sleeps_until_expiration_then_animates_its_exit() {
    let clock = VirtualClock::default();
    let mut view = View::default();
    view.toasts.toast(Toast::message("Saved"));
    let mut source = Source::with_clock(view, clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert!(source.has_anim());
    clock.set(Duration::from_millis(140)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert!(
        !source.has_anim(),
        "the lifetime is a timer, not an animation"
    );
    let deadline = source
        .animation_deadline()
        .expect("expiration must wake the host");
    for ms in [500, 1500, 3999] {
        clock.set(Duration::from_millis(ms)).unwrap();
        source
            .render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
            .unwrap();
        assert_eq!(source.view().toasts.len(), 1);
        assert!(!source.has_anim());
        assert_eq!(source.frame_diagnostics().backend.sync_visits, 0);
        assert_eq!(source.animation_deadline(), Some(deadline));
    }
    clock.set(Duration::from_secs(4)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert!(source.view().toasts.is_empty());
    assert!(source.has_anim());
    clock.advance(Duration::from_millis(140)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text, (500, 300), 1.)
        .unwrap();
    assert!(!source.has_anim());
    assert_eq!(source.animation_deadline(), None);
}
