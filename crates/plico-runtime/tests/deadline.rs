use plico::VirtualClock;
use plico::prelude::*;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{HostView, Source};
use std::time::Duration;
#[derive(Default)]
struct Timer {
    declarations: usize,
    hidden: bool,
    moving: bool,
}
impl HostView for Timer {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.declarations += 1;
        if !self.hidden {
            column(cx)
                .scale(
                    if self.moving { 1.2 } else { 1. },
                    if self.moving { 1.2 } else { 1. },
                )
                .transition(100)
                .children(|cx| {
                    for second in [3, 1, 2] {
                        let deadline = Duration::from_secs(second);
                        if cx.now() < deadline {
                            cx.request_rebuild_at(deadline);
                        }
                    }
                    text(cx, "Timer");
                });
        }
    }
}
fn render(s: &mut Source<Timer>, t: &mut TextContext, scene: &mut Scene) {
    s.render_retained_frame(scene, t, (300, 200), 1.).unwrap();
}
#[test]
fn earliest_deadline_retains_declarations_until_due_then_cancels() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Timer::default(), clock.clone());
    let mut t = TextContext::new();
    let mut scene = Scene::new();
    render(&mut s, &mut t, &mut scene);
    let initial = s.view().declarations;
    assert!(
        !s.has_anim(),
        "a future deadline must not start a vsync loop"
    );
    let first_wake = s.animation_deadline().expect("timer needs a host wake");
    for ms in [100, 500, 999] {
        clock.set(Duration::from_millis(ms)).unwrap();
        render(&mut s, &mut t, &mut scene);
        assert_eq!(s.view().declarations, initial);
        assert_eq!(s.frame_diagnostics().backend.sync_visits, 0);
        assert!(!s.has_anim());
        assert_eq!(
            s.animation_deadline(),
            Some(first_wake),
            "sampling must not postpone the host deadline"
        );
    }
    for second in 1..=3 {
        clock.set(Duration::from_secs(second)).unwrap();
        render(&mut s, &mut t, &mut scene);
        assert_eq!(s.view().declarations, initial + second as usize);
        assert!(!s.has_anim());
        assert_eq!(
            s.animation_deadline(),
            (second < 3).then(|| first_wake + Duration::from_secs(second))
        );
    }
    assert!(!s.has_anim());
    assert_eq!(s.animation_deadline(), None);
}
#[test]
fn removing_the_owner_cancels_its_deadline() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Timer::default(), clock.clone());
    let mut t = TextContext::new();
    let mut scene = Scene::new();
    render(&mut s, &mut t, &mut scene);
    s.update(|view| view.hidden = true);
    render(&mut s, &mut t, &mut scene);
    assert_eq!(s.animation_deadline(), None);
    assert!(!s.has_anim());
    let declarations = s.view().declarations;
    clock.set(Duration::from_secs(5)).unwrap();
    render(&mut s, &mut t, &mut scene);
    assert_eq!(s.view().declarations, declarations);
}

#[test]
fn timer_deadlines_do_not_suppress_real_presentation_animation() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Timer::default(), clock.clone());
    let mut t = TextContext::new();
    let mut scene = Scene::new();
    render(&mut s, &mut t, &mut scene);
    let deadline = s.animation_deadline().unwrap();
    s.update(|view| view.moving = true);
    render(&mut s, &mut t, &mut scene);
    assert!(s.has_anim());
    assert_eq!(s.animation_deadline(), Some(deadline));
    clock.advance(Duration::from_millis(200)).unwrap();
    render(&mut s, &mut t, &mut scene);
    assert!(!s.has_anim());
    assert_eq!(s.animation_deadline(), Some(deadline));
}

#[derive(Default)]
struct DeclarativeMotion {
    declarations: usize,
    target: f32,
    sampled: f32,
}
impl HostView for DeclarativeMotion {
    fn show(&mut self, cx: &mut UiContext) {
        self.declarations += 1;
        self.sampled = cx.animate("width", self.target, Duration::from_millis(100));
        column(cx).width(self.sampled);
    }
}

#[test]
fn declarative_animation_runs_at_host_frames_without_a_fixed_deadline() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(DeclarativeMotion::default(), clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (300, 200), 1.)
        .unwrap();
    source.update(|view| view.target = 100.);
    source
        .render_retained_frame(&mut scene, &mut text, (300, 200), 1.)
        .unwrap();
    let started = source.view().declarations;
    assert!(source.has_anim());
    assert_eq!(source.animation_deadline(), None);

    clock.advance(Duration::from_millis(50)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text, (300, 200), 1.)
        .unwrap();
    assert_eq!(source.view().declarations, started + 1);
    assert!((source.view().sampled - 87.5).abs() < 1e-4);
    assert!(source.has_anim());

    clock.advance(Duration::from_millis(50)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text, (300, 200), 1.)
        .unwrap();
    assert_eq!(source.view().sampled, 100.);
    assert!(!source.has_anim());
}
