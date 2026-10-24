use plico_core::{Clock, FrameTime, UiContext, VirtualClock, component};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_vello::{Source, View};
use std::time::Duration;
#[derive(Default)]
struct Timed {
    seen: Vec<FrameTime>,
    count: u32,
}
impl View for Timed {
    #[component]
    fn show(&mut self, ui: &mut UiContext<'_>) {
        self.seen.push(ui.time());
        ui.text(format!("Time: {}", ui.now().as_millis()));
        if ui.button("Increment").clicked() {
            self.count += 1;
        }
        if ui.now() < Duration::from_millis(200) {
            ui.request_animation_frame();
        }
    }
}
#[test]
fn injected_clock_drives_ui_pause_and_future_frames_without_sleep() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Timed::default(), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 600, 300);
    assert_eq!(s.view.seen[0], FrameTime::default());
    assert!(s.has_anim());
    clock.advance(Duration::from_millis(100)).unwrap();
    let nodes = s.build_frame(&mut text, 600, 300);
    assert_eq!(
        s.view.seen.last().unwrap().delta,
        Duration::from_millis(100)
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.paint.text.as_deref() == Some("Time: 100"))
    );
    s.build_frame(&mut text, 600, 300);
    assert_eq!(s.view.seen.last().unwrap().delta, Duration::ZERO);
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Increment", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Click { target: id });
    let before = s.view.seen.len();
    s.build_frame(&mut text, 600, 300);
    assert_eq!(s.view.count, 1);
    assert_eq!(s.view.seen.len(), before + 2);
    assert_eq!(s.view.seen[before], s.view.seen[before + 1]);
    clock.advance(Duration::from_millis(100)).unwrap();
    s.build_frame(&mut text, 600, 300);
    assert!(!s.has_anim());
}
#[test]
fn injected_clock_drives_host_layout_tweens_too() {
    struct BoxView(bool);
    impl View for BoxView {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            ui.text("Box")
                .width(if self.0 { 300. } else { 100. })
                .transition(200);
        }
    }
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(BoxView(false), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 600, 300);
    s.view.0 = true;
    s.request_rebuild();
    s.build_frame(&mut text, 600, 300);
    clock.advance(Duration::from_millis(100)).unwrap();
    let nodes = s.build_frame(&mut text, 600, 300);
    let p = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Box"))
        .unwrap();
    assert_eq!(p.rect[2] - p.rect[0], 200.);
    clock.advance(Duration::from_millis(100)).unwrap();
    s.build_frame(&mut text, 600, 300);
    assert!(!s.has_anim());
}
#[test]
fn external_clock_trait_does_not_require_instant() {
    struct GameClock;
    impl Clock for GameClock {
        fn now(&self) -> Duration {
            Duration::from_secs(3)
        }
    }
    let mut s = Source::with_clock(Timed::default(), GameClock);
    let mut text = TextContext::new();
    s.build_frame(&mut text, 600, 300);
    assert_eq!(s.view.seen[0].now, Duration::from_secs(3));
    assert_eq!(s.view.seen[0].delta, Duration::ZERO);
    assert!(!s.has_anim());
}
