use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{FrameSource, TextContext};

#[derive(Default)]
struct Demo {
    extra: bool,
    reversed: bool,
}
impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        let _ = column(cx).children(|cx| {
            for i in 0..1000 {
                text(cx, "Stable").key(i as u64);
            }
            column(cx).key("dynamic").children(|cx| {
                if self.reversed {
                    button(cx).text("B").key("b");
                    button(cx).text("A").key("a");
                } else {
                    button(cx).text("A").key("a");
                    button(cx).text("B").key("b");
                }
                if self.extra {
                    let _ = column(cx).key("extra").children(|cx| {
                        text(cx, "New child");
                    });
                }
            })
        });
    }
}
#[test]
fn insert_remove_and_reorder_do_not_sync_static_siblings() {
    let mut source = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 800, 600);
    assert!(source.frame_diagnostics().backend.sync_visits > 1000);
    source.update(|view| view.extra = true);
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.frame_diagnostics().backend.created, 2);
    assert_eq!(source.frame_diagnostics().backend.sync_visits, 3);
    source.update(|view| view.reversed = true);
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.frame_diagnostics().backend.sync_visits, 1);
    assert_eq!(source.frame_diagnostics().backend.child_writes, 1);
    source.update(|view| view.extra = false);
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.frame_diagnostics().backend.removed, 2);
    assert_eq!(source.frame_diagnostics().backend.sync_visits, 1);
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.frame_diagnostics().backend.created, 0);
}
