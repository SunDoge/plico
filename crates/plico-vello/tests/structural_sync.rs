use plico_core::prelude::*;
use plico_renderer::{FrameSource, TextContext};
use plico_vello::{Source, View};

#[derive(Default)]
struct Demo {
    extra: bool,
    reversed: bool,
}
impl View for Demo {
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
    // The compatibility layout entry explicitly requests a full synchronization.
    source
        .adapter
        .layout(&mut source.engine, &mut tcx, 800, 600);
    assert!(source.adapter.stats.sync_visits > 1000);
    source.view.extra = true;
    source.request_rebuild();
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.adapter.stats.created, 2);
    assert_eq!(source.adapter.stats.sync_visits, 3);
    source.view.reversed = true;
    source.request_rebuild();
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.adapter.stats.sync_visits, 1);
    assert_eq!(source.adapter.stats.child_writes, 1);
    source.view.extra = false;
    source.request_rebuild();
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.adapter.stats.removed, 2);
    assert_eq!(source.adapter.stats.sync_visits, 1);
    source.build_frame(&mut tcx, 800, 600);
    assert_eq!(source.adapter.stats.created, 0);
}

#[test]
fn full_layout_reconciles_removals_after_multiple_simulated_frames() {
    let mut engine = plico_core::Engine::default();
    let mut adapter = plico_vello::Adapter::default();
    let mut text = TextContext::new();
    engine
        .frame(|ui| {
            ui.text("Keep").key("keep");
            ui.text("Remove").key("remove");
        })
        .unwrap();
    adapter.layout(&mut engine, &mut text, 300, 200);
    let removed = engine.find(engine.root(), "remove").unwrap();
    assert!(adapter.backend_id(removed).is_some());
    for _ in 0..2 {
        engine
            .frame(|ui| {
                ui.text("Keep").key("keep");
            })
            .unwrap();
    }
    adapter.layout(&mut engine, &mut text, 300, 200);
    assert!(adapter.backend_id(removed).is_none());
    assert_eq!(adapter.stats.removed, 1);
}
