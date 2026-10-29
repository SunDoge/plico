use plico::prelude::*;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{
    Source,
    regions::{Regions, WatchUi},
};
fn snapshot(full: bool) -> Vec<(String, [f32; 4])> {
    let mut state = None;
    let app = Regions::new(|app| {
        let gap = app.state(|| false);
        state = Some(gap);
        app.root(gap, |cx, gap| {
            cx.watch(gap.handle(), |cx, gap| {
                row(cx)
                    .width(400.)
                    .height(80.)
                    .gap(if *gap.read() { 20. } else { 0. })
                    .children(|cx| {
                        column(cx).key(1_u64).width(0.).grow(1.).children(|cx| {
                            text(cx, "a");
                        });
                        column(cx).key(2_u64).width(0.).grow(1.).children(|cx| {
                            text(cx, "b");
                        });
                        column(cx).key(3_u64).width(0.).grow(1.).children(|cx| {
                            text(cx, "c");
                        });
                    });
            });
        });
    });
    let mut source = Source::new(app);
    source.adapter.experiment_force_full_geometry = full;
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (500, 200), 1.)
        .unwrap();
    source.view.access(state.unwrap()).set(true);
    source
        .render_retained_frame(&mut scene, &mut text, (500, 200), 1.)
        .unwrap();
    source
        .adapter
        .placed
        .iter()
        .filter_map(|p| p.paint.text.as_ref().map(|t| (t.to_string(), p.rect)))
        .collect()
}
#[test]
fn fixed_parent_gap_reflows_growing_children() {
    let incremental = snapshot(false);
    assert_eq!(incremental, snapshot(true));
    let positions: Vec<_> = incremental.iter().map(|(_, rect)| rect[0]).collect();
    assert_eq!(positions, [0., 140., 280.]);
}

#[test]
fn structural_replay_reuses_independent_scene_regions() {
    #[derive(Clone, Copy)]
    struct States {
        visible: plico_runtime::regions::State<bool>,
        reverse: plico_runtime::regions::State<bool>,
        sibling: plico_runtime::regions::State<u32>,
    }
    let mut states = None;
    let app = Regions::new(|app| {
        let current = States {
            visible: app.state(|| true),
            reverse: app.state(|| false),
            sibling: app.state(|| 0),
        };
        states = Some(current);
        let current = app.state(|| current);
        app.root(current, |cx, current| {
            let current = *current.read();
            column(cx).children(|cx| {
                column(cx).height(57.).children(|cx| {
                    cx.when(
                        (current.visible, current.reverse),
                        |(visible, _)| *visible.read(),
                        |cx, (_, reverse)| {
                            cx.each(
                                reverse.handle(),
                                |reverse| {
                                    let mut rows = vec![0_u64, 1, 2];
                                    if *reverse.read() {
                                        rows.reverse();
                                    }
                                    rows
                                },
                                |cx, _, row| {
                                    text!(cx, "Row {row}");
                                },
                            );
                        },
                    );
                    cx.when(
                        current.visible,
                        |visible| !*visible.read(),
                        |cx, _| {
                            text(cx, "Hidden");
                        },
                    );
                });
                cx.watch(current.sibling, |cx, sibling| {
                    text!(cx, "Sibling {}", *sibling.read());
                });
            });
        });
    });
    let states = states.unwrap();
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    let mut frame = |source: &mut Source<Regions>| {
        source
            .render_retained_frame(&mut scene, &mut text, (400, 300), 1.)
            .unwrap();
        let mut reference = Scene::new();
        let surface = source.engine.theme().surface;
        let base =
            peniko::Color::from_rgb8((surface >> 16) as u8, (surface >> 8) as u8, surface as u8);
        plico_renderer::scene::build_scene_scaled_layers(
            &mut reference,
            &[source.adapter.placed.as_slice(), &[]],
            &mut text,
            400,
            300,
            base,
            1.,
        );
        assert_eq!(scene.flattened().commands, reference.commands);
        source.adapter.stats
    };
    frame(&mut source);
    for update in [
        Box::new(|app: &Regions| app.access(states.reverse).set(true)) as Box<dyn Fn(&Regions)>,
        Box::new(|app: &Regions| app.access(states.visible).set(false)),
        Box::new(|app: &Regions| app.access(states.visible).set(true)),
        Box::new(|app: &Regions| app.access(states.sibling).set(1)),
    ] {
        update(&source.view);
        let stats = frame(&mut source);
        assert_eq!(stats.scene_full_rebuilds, 0, "{stats:?}");
        assert!(stats.scene_fragment_reuses > 0, "{stats:?}");
        if stats.child_writes > 0 {
            assert!(stats.flatten_patches > 0, "{stats:?}");
            assert!(
                stats.flatten_visits < source.adapter.placed.len(),
                "{stats:?}"
            );
        }
    }
}

#[test]
fn semantic_projection_reuses_snapshot_and_clean_node_records() {
    let mut state = None;
    let app = Regions::new(|app| {
        let value = app.state(|| 0_u32);
        state = Some(value);
        app.root(value, |cx, value| {
            column(cx).children(|cx| {
                cx.watch(value.handle(), |cx, value| {
                    text!(cx, "Value {}", *value.read());
                });
                text(cx, "Static sibling");
            });
        });
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    let first = source.adapter.semantics_shared(&source.engine, 1);
    let node_count = first.nodes.len();
    let reused = source.adapter.semantics_shared(&source.engine, 1);
    assert!(std::sync::Arc::ptr_eq(&first, &reused));

    source.view.access(state.unwrap()).set(1);
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    let changed = source.adapter.semantics_shared(&source.engine, 2);
    assert!(!std::sync::Arc::ptr_eq(&first, &changed));
    assert!(
        changed
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("Value 1"))
    );
    let (builds, reuses, snapshot_reuses) = source.adapter.experiment_semantic_cache_stats();
    assert!(builds < node_count, "builds={builds} nodes={node_count}");
    assert!(reuses > 0);
    assert_eq!(snapshot_reuses, 1);
}
