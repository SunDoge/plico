use plico::VirtualClock;
use plico::prelude::*;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{HostView, Source};
use std::time::Duration;
#[derive(Default)]
struct Timer {
    declarations: usize,
    hidden: bool,
}
impl HostView for Timer {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.declarations += 1;
        if !self.hidden {
            column(cx).children(|cx| {
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
    assert!(s.last_error.is_none());
}
#[test]
fn earliest_deadline_retains_declarations_until_due_then_cancels() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Timer::default(), clock.clone());
    let mut t = TextContext::new();
    let mut scene = Scene::new();
    render(&mut s, &mut t, &mut scene);
    let initial = s.view.declarations;
    for ms in [100, 500, 999] {
        clock.set(Duration::from_millis(ms)).unwrap();
        render(&mut s, &mut t, &mut scene);
        assert_eq!(s.view.declarations, initial);
        assert_eq!(s.adapter.stats.sync_visits, 0);
    }
    for second in 1..=3 {
        clock.set(Duration::from_secs(second)).unwrap();
        render(&mut s, &mut t, &mut scene);
        assert_eq!(s.view.declarations, initial + second as usize);
    }
    assert!(!s.has_anim());
    assert_eq!(s.engine.wake_deadline(), None);
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
    assert_eq!(s.engine.wake_deadline(), None);
    assert!(!s.has_anim());
    let declarations = s.view.declarations;
    clock.set(Duration::from_secs(5)).unwrap();
    render(&mut s, &mut t, &mut scene);
    assert_eq!(s.view.declarations, declarations);
}
