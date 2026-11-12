use super::Adapter;
use plico::{Engine, Kind, UiContext};
use plico_renderer::{TextContext, layout::PlacedNode};
use std::{collections::HashMap, sync::Arc};

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

#[test]
fn asset_cache_pruning_is_linear_and_keeps_recently_unmounted_sources() {
    const SOURCE: &str =
        r#"<svg viewBox="0 0 16 16"><path fill="currentColor" d="M0 0h16v16H0z"/></svg>"#;
    let mut engine = Engine::default();
    let mut adapter = Adapter::default();
    let mut text = TextContext::new();
    engine
        .frame(|ui| {
            ui.svg_source(SOURCE.to_owned());
        })
        .unwrap();
    layout(&mut adapter, &mut engine, &mut text, 300, 200);

    for color in 0..128 {
        super::cached_svg(
            &mut adapter.svg_cache,
            &mut adapter.stats,
            1,
            SOURCE,
            Some(color),
        );
    }
    adapter.resource_generation = 32;
    adapter.stats = Default::default();
    adapter.prune_asset_caches();
    let variants = adapter.svg_cache.values().map(HashMap::len).sum::<usize>();
    assert_eq!(variants, 1, "only the live currentColor variant remains");
    assert_eq!(adapter.stats.resource_prune_visits, adapter.nodes.len());
    assert!(adapter.stats.resource_evictions >= 127);

    engine.frame(|_| {}).unwrap();
    layout(&mut adapter, &mut engine, &mut text, 300, 200);
    assert_eq!(
        adapter.svg_cache.values().map(HashMap::len).sum::<usize>(),
        1,
        "a just-unmounted asset stays warm"
    );
    adapter.resource_generation = adapter.resource_generation.wrapping_add(31);
    adapter.prune_asset_caches();
    assert!(adapter.svg_cache.is_empty());
}

#[test]
fn layout_cache_walk_reuses_one_iterative_stack() {
    let mut engine = Engine::default();
    let mut adapter = Adapter::default();
    let mut text = TextContext::new();
    engine
        .frame(|ui| {
            for id in 0_u64..64 {
                ui.text("row").key(id);
            }
        })
        .unwrap();
    layout(&mut adapter, &mut engine, &mut text, 800, 600);
    for retained in adapter.nodes.values_mut() {
        retained.layout = None;
    }
    adapter.layout_cache_scratch = Vec::with_capacity(128);
    let storage = adapter.layout_cache_scratch.as_ptr();
    let root = adapter.nodes[&engine.root()].id;
    adapter.cache_layout_subtree(root);
    assert!(adapter.nodes.values().all(|node| node.layout.is_some()));
    assert_eq!(adapter.layout_cache_scratch.as_ptr(), storage);

    for retained in adapter.nodes.values_mut() {
        retained.layout = None;
    }
    adapter.cache_layout_subtree(root);
    assert!(adapter.nodes.values().all(|node| node.layout.is_some()));
    assert_eq!(adapter.layout_cache_scratch.as_ptr(), storage);
}

#[test]
#[ignore = "manual CPU profile"]
fn profile_linear_asset_cache_liveness() {
    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    }

    let mut engine = Engine::default();
    let mut adapter = Adapter::default();
    let mut text = TextContext::new();
    engine
        .frame(|ui| {
            for id in 0_u64..4_000 {
                ui.text("row").key(id);
            }
        })
        .unwrap();
    layout(&mut adapter, &mut engine, &mut text, 800, 600);
    let stale: Vec<Arc<str>> = (0..256)
        .map(|id| Arc::from(format!("<svg data-id='{id}'/>")))
        .collect();
    for source in &stale {
        super::cached_svg(&mut adapter.svg_cache, &mut adapter.stats, 1, source, None);
    }
    adapter.resource_generation = 1;
    let mut nested = Vec::new();
    let mut linear = Vec::new();
    const ITERATIONS: usize = 10;
    for sample in 0..10 {
        let started = std::time::Instant::now();
        for _ in 0..ITERATIONS {
            if sample % 2 == 0 {
                for source in &stale {
                    std::hint::black_box(
                        adapter
                            .nodes
                            .values()
                            .any(|node| node.paint_key.kind == Kind::Svg && node.text == *source),
                    );
                }
            } else {
                adapter.prune_asset_caches();
                std::hint::black_box(&adapter.svg_cache);
            }
        }
        let elapsed = started.elapsed().as_secs_f64() * 1e6 / ITERATIONS as f64;
        if sample % 2 == 0 {
            nested.push(elapsed);
        } else {
            linear.push(elapsed);
        }
    }
    eprintln!(
        "4001 nodes / 256 stale assets us/pass: nested={:.2} linear={:.2}",
        median(&mut nested),
        median(&mut linear),
    );
}
