use super::Adapter;
use plico::{Engine, UiContext};
use plico_renderer::{TextContext, layout::PlacedNode};

fn layout(
    adapter: &mut Adapter,
    engine: &mut Engine,
    text: &mut TextContext,
    width: u32,
    height: u32,
) -> Vec<PlacedNode> {
    adapter.update_retained(engine, text, width, height, &[], plico::Changes::STRUCTURE);
    adapter.presented()
}

#[test]
fn retained_nodes_survive_reorder_and_are_recreated_after_remount() {
    let mut engine = Engine::default();
    let mut adapter = Adapter::default();
    let mut text = TextContext::new();
    let draw = |ui: &mut UiContext<'_>, ids: &[u64]| {
        for id in ids {
            ui.text("item").key(*id);
        }
    };

    engine.frame_declarations(|ui| draw(ui, &[1, 2])).unwrap();
    layout(&mut adapter, &mut engine, &mut text, 640, 780);
    let one = engine.find(engine.root(), 1u64).unwrap();
    let two = engine.find(engine.root(), 2u64).unwrap();
    let first = adapter.backend_id(one);
    let second = adapter.backend_id(two);

    engine.frame_declarations(|ui| draw(ui, &[2, 1])).unwrap();
    layout(&mut adapter, &mut engine, &mut text, 640, 780);
    assert_eq!(adapter.backend_id(one), first);
    assert_eq!(adapter.backend_id(two), second);
    assert_eq!(adapter.stats.created, 0);
    assert_eq!(adapter.stats.child_writes, 1);

    engine.frame_declarations(|ui| draw(ui, &[2])).unwrap();
    layout(&mut adapter, &mut engine, &mut text, 640, 780);
    assert_eq!(adapter.stats.removed, 1);
    assert!(adapter.backend_id(one).is_none());

    engine.frame_declarations(|ui| draw(ui, &[2, 1])).unwrap();
    layout(&mut adapter, &mut engine, &mut text, 640, 780);
    let new_one = engine.find(engine.root(), 1u64).unwrap();
    assert_ne!(new_one, one);
    assert_ne!(adapter.backend_id(new_one), first);
    assert_eq!(adapter.backend_id(two), second);
}

#[test]
fn full_layout_reconciles_removals_after_skipped_projection_frames() {
    let mut engine = Engine::default();
    let mut adapter = Adapter::default();
    let mut text = TextContext::new();
    engine
        .frame(|ui| {
            ui.text("Keep").key("keep");
            ui.text("Remove").key("remove");
        })
        .unwrap();
    layout(&mut adapter, &mut engine, &mut text, 300, 200);
    let removed = engine.find(engine.root(), "remove").unwrap();
    assert!(adapter.backend_id(removed).is_some());

    for _ in 0..2 {
        engine
            .frame(|ui| {
                ui.text("Keep").key("keep");
            })
            .unwrap();
    }
    layout(&mut adapter, &mut engine, &mut text, 300, 200);
    assert!(adapter.backend_id(removed).is_none());
    assert_eq!(adapter.stats.removed, 1);
}
