use std::time::Duration;

use plico::prelude::*;
use plico::{Engine, FrameTime};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Home,
    Settings,
}

struct TestApp {
    router: Router<Page>,
    calls: [usize; 2],
    local_initializations: usize,
}

impl Default for TestApp {
    fn default() -> Self {
        let mut router = Router::new(Page::Home);
        router.transition = RouterTransition::Slide;
        Self {
            router,
            calls: [0; 2],
            local_initializations: 0,
        }
    }
}

impl TestApp {
    fn show(&mut self, cx: &mut UiContext<'_>) {
        // `view` returns an owned snapshot: this closure can borrow the rest of
        // `self` without fighting a live mutable borrow of `self.router`.
        self.router.view().show(cx, |cx, page| {
            let index = usize::from(page == Page::Settings);
            self.calls[index] += 1;
            let local = cx.state(|| {
                self.local_initializations += 1;
                7_u32
            });
            column(cx).key("content").children(|cx| {
                text(
                    cx,
                    if page == Page::Home {
                        "Home"
                    } else {
                        "Settings"
                    },
                );
                text(cx, local.read().to_string());
            });
        });
    }
}

fn frame(engine: &mut Engine, app: &mut TestApp, now: u64) {
    engine.set_time(FrameTime {
        now: Duration::from_millis(now),
        delta: Duration::from_millis(16),
    });
    engine.frame_declarations(|cx| app.show(cx)).unwrap();
}

#[test]
fn page_host_builds_both_pages_only_during_the_transition() {
    let mut engine = Engine::default();
    let mut app = TestApp::default();
    frame(&mut engine, &mut app, 0);
    assert_eq!(app.calls, [1, 0]);
    assert_eq!(app.local_initializations, 1);

    app.router.go(Page::Settings);
    frame(&mut engine, &mut app, 16);
    assert_eq!(
        app.calls,
        [2, 1],
        "the inert leaving page and new page build together"
    );
    assert_eq!(app.local_initializations, 2);

    frame(&mut engine, &mut app, 300);
    assert_eq!(
        app.calls,
        [2, 2],
        "a hidden history page is retained without rebuilding"
    );

    app.router.back();
    frame(&mut engine, &mut app, 316);
    assert_eq!(app.calls, [3, 3]);
    assert_eq!(
        app.local_initializations, 2,
        "Back remounts neither page-local state slot"
    );
}

#[test]
fn reduced_motion_replaces_the_page_atomically() {
    let mut engine = Engine::default();
    engine.set_display_preferences(plico::DisplayPreferences {
        reduce_motion: true,
        ..Default::default()
    });
    let mut app = TestApp::default();
    frame(&mut engine, &mut app, 0);
    app.router.go(Page::Settings);
    frame(&mut engine, &mut app, 16);
    assert_eq!(
        app.calls,
        [1, 1],
        "the old page is retained but not rebuilt"
    );
}
