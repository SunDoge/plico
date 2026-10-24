use plico_core::{Align, Justify, UiContext};
use plico_renderer::{FrameSource, TextContext};
use plico_vello::{Source, View};
struct Flex {
    align: Align,
    wrap: bool,
}
impl View for Flex {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        let _ = ui
            .row()
            .key("row")
            .width_percent(100.)
            .height(160.)
            .gap(10)
            .align(self.align)
            .justify(Justify::SpaceBetween)
            .wrap(self.wrap)
            .children(|ui| {
                ui.column()
                    .key("fixed")
                    .width(80.)
                    .shrink(0.)
                    .height(30.)
                    .children(|_ui| {});
                ui.column()
                    .key("grow")
                    .width(0.)
                    .grow(1.)
                    .height(50.)
                    .children(|_ui| {});
                let _ = ui
                    .column()
                    .key("ratio")
                    .width_percent(25.)
                    .shrink(0.)
                    .height(70.)
                    .children(|_ui| {});
            });
    }
}
fn rect(s: &Source<Flex>, key: &str) -> [f32; 4] {
    // Fixture keys, not application identity generation.
    let row = s.engine.find(s.engine.root(), "row").unwrap();
    let key = match key {
        "fixed" => "fixed",
        "grow" => "grow",
        _ => "ratio",
    };
    let entity = s.engine.find(row, key).unwrap();
    let id = s.adapter.backend_id(entity).unwrap();
    s.adapter
        .placed
        .iter()
        .find(|n| n.node_id == id)
        .unwrap()
        .rect
}
#[test]
fn fixed_grow_and_percentage_share_space_and_resize() {
    let mut s = Source::new(Flex {
        align: Align::Start,
        wrap: false,
    });
    let mut tcx = TextContext::new();
    for width in [400, 800] {
        s.build_frame(&mut tcx, width, 200);
        assert!(s.last_error.is_none());
        let fixed = rect(&s, "fixed");
        let grow = rect(&s, "grow");
        let ratio = rect(&s, "ratio");
        assert_eq!(fixed[2] - fixed[0], 80.);
        assert_eq!(ratio[2] - ratio[0], width as f32 / 4.);
        assert_eq!(grow[2] - grow[0], width as f32 * 0.75 - 100.);
        assert_eq!(ratio[2], width as f32);
    }
}
#[test]
fn cross_axis_changes_invalidate_geometry_and_settle() {
    let mut s = Source::new(Flex {
        align: Align::Start,
        wrap: false,
    });
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 200);
    assert_eq!(rect(&s, "fixed")[1], 0.);
    s.view.align = Align::End;
    s.build_frame(&mut tcx, 400, 200);
    assert!(s.engine.changes().contains(plico_core::Changes::LAYOUT));
    assert_eq!(s.adapter.stats.layout_runs, 1);
    assert_eq!(rect(&s, "fixed")[1], 130.);
    s.build_frame(&mut tcx, 400, 200);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(s.adapter.stats.sync_visits, 0);
}
struct Wrap(bool);
impl View for Wrap {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        let _ = ui.row().width(180.).gap(10).wrap(self.0).children(|ui| {
            for i in 0..3usize {
                let _ = ui.column().key(i).width(80.).height(30.).children(|ui| {
                    ui.text(format!("item-{i}"));
                });
            }
        });
    }
}
#[test]
fn wrap_reflows_real_children_instead_of_manually_positioning_them() {
    let mut s = Source::new(Wrap(false));
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 300, 200);
    let y = |s: &Source<Wrap>, label: &str| {
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(plico_renderer::SemanticRole::Label, label, None)
            .unwrap()
            .bounds[1]
    };
    assert_eq!(y(&s, "item-0"), y(&s, "item-2"));
    s.view.0 = true;
    s.build_frame(&mut tcx, 300, 200);
    assert!(y(&s, "item-2") > y(&s, "item-0"));
}
