use plico::VirtualClock;
use plico::prelude::*;
use plico_renderer::{FrameSource, Scene, SemanticRole, TextContext, UiEvent, WindowMetrics};
use plico_runtime::{HostView, Source};
use std::time::Duration;

#[derive(Default)]
struct Demo {
    calls: usize,
    seen: DisplayPreferences,
    x: f32,
    animate_glass: bool,
    modal: ModalState,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        self.seen = cx.display_preferences();
        column(cx)
            .width(200)
            .height(100)
            .material(LiquidGlass {
                animated: self.animate_glass,
                ..LiquidGlass::regular()
            })
            .children(|cx| {
                column(cx)
                    .translate(self.x, 0.)
                    .transition(200)
                    .children(|cx| {
                        button(cx).text("Target").width(80).height(32);
                    });
            });
        modal(cx, &mut self.modal)
            .title("Accessible dialog")
            .children(|cx| {
                text(cx, "Complete content");
            });
    }
}
fn frame<V: HostView>(s: &mut Source<V>, text: &mut TextContext) -> Scene {
    let mut scene = Scene::new();
    s.render_retained_frame(&mut scene, text, (500, 350), 1.)
        .unwrap();
    scene
}
fn preferences<V: HostView>(s: &mut Source<V>, p: DisplayPreferences) -> bool {
    s.handle_event(UiEvent::WindowMetrics(WindowMetrics {
        reduced_motion: p.reduce_motion,
        reduced_transparency: p.reduce_transparency,
        high_contrast: p.high_contrast,
        ..Default::default()
    }))
    .request_redraw
}
fn shaders(scene: &Scene) -> usize {
    scene
        .flattened()
        .commands
        .iter()
        .filter(|command| matches!(command, plico_renderer::PaintCommand::Shader { .. }))
        .count()
}
#[test]
fn system_preferences_replace_glass_and_stop_motion_without_overwriting_app_policy() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(
        Demo {
            animate_glass: true,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    let mut scene = frame(&mut s, &mut text);
    assert!(shaders(&scene) > 0);
    assert!(s.has_anim());
    let requested = DisplayPreferences {
        reduce_motion: true,
        reduce_transparency: true,
        high_contrast: true,
    };
    assert!(preferences(&mut s, requested));
    scene = frame(&mut s, &mut text);
    assert_eq!(s.view().seen, requested);
    assert_eq!(
        shaders(&scene),
        0,
        "opaque fallback must remove backdrop work"
    );
    assert!(!s.has_anim());
    assert_ne!(s.theme().border, Theme::default().border);
    let calls = s.view().calls;
    assert!(!preferences(&mut s, requested));
    frame(&mut s, &mut text);
    assert_eq!(
        s.view().calls,
        calls,
        "identical metrics must not redeclare"
    );
    s.set_reduced_motion(false);
    s.update(|view| view.x = 100.);
    frame(&mut s, &mut text);
    assert!(
        !s.has_anim(),
        "app false must not override the native reduce-motion request"
    );
    let bounds = s
        .semantics()
        .node_by_role(SemanticRole::Button, "Target", None)
        .unwrap()
        .bounds;
    assert_eq!(bounds[0], 100.);
    s.update(|view| view.modal.open());
    frame(&mut s, &mut text);
    assert!(!s.has_anim());
    s.update(|view| view.modal.close());
    frame(&mut s, &mut text);
    assert!(
        s.semantics()
            .node_by_role(SemanticRole::Dialog, "Accessible dialog", None)
            .is_none(),
        "system reduction skips modal exit retention too"
    );
    s.set_reduced_motion(true);
    assert!(preferences(&mut s, DisplayPreferences::default()));
    scene = frame(&mut s, &mut text);
    assert!(
        shaders(&scene) > 0,
        "turning native transparency reduction off restores material"
    );
    s.update(|view| {
        view.animate_glass = false;
        view.x = 200.;
    });
    frame(&mut s, &mut text);
    assert!(
        !s.has_anim(),
        "turning the system setting off must retain app reduction"
    );
    s.set_reduced_motion(false);
    s.update(|view| view.x = 250.);
    frame(&mut s, &mut text);
    assert!(s.has_anim());
    clock.advance(Duration::from_millis(250)).unwrap();
    frame(&mut s, &mut text);
    assert!(!s.has_anim());
    assert_eq!(s.theme(), Theme::default());
}

#[test]
fn native_preferences_refresh_the_immediate_view_with_pending_state() {
    #[derive(Default)]
    struct App(u32);
    impl HostView for App {
        fn show(&mut self, cx: &mut UiContext<'_>) {
            column(cx)
                .width(100)
                .height(60)
                .material(LiquidGlass::regular())
                .end();
            text!(cx, "Count {}", self.0);
        }
    }
    let app = App::default();
    let mut s = Source::with_clock(app, VirtualClock::default());
    let mut text = TextContext::new();
    let mut scene = frame(&mut s, &mut text);
    assert!(shaders(&scene) > 0);
    s.update(|view| view.0 += 1);
    preferences(
        &mut s,
        DisplayPreferences {
            reduce_transparency: true,
            ..Default::default()
        },
    );
    scene = frame(&mut s, &mut text);
    assert_eq!(
        shaders(&scene),
        0,
        "the complete view must follow native preferences"
    );
    assert!(
        s.placed_nodes()
            .iter()
            .any(|node| node.paint.text.as_deref() == Some("Count 1"))
    );
}
