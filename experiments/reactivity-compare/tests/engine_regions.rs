use plico::{Engine, Entity, FrameError};
use plico_reactivity_compare::{
    ComponentId, RegionId, Runtime, State, engine_bridge::EngineBridge,
};
use plico_renderer::TextContext;

struct Fixture {
    runtime: Runtime,
    component: ComponentId,
    root: RegionId,
    visible: State<bool>,
    title: State<bool>,
    value: State<String>,
}
impl Fixture {
    fn new() -> Self {
        let mut runtime = Runtime::default();
        let component = runtime.component();
        let visible = runtime.state_in(component, 1, || true);
        let title = runtime.state_in(component, 2, || false);
        let value = runtime.state_in(component, 3, || String::from("short"));
        let root = runtime.watch_in(component, (visible, title), |cx, visible, title| {
            cx.text(format!("Title {}", *title.read()));
            if *visible.read() {
                cx.watch(10, (cx.state::<String>(3),), |cx, value| {
                    cx.text(format!("Value {}", *value.read()));
                    cx.watch(20, (value.handle(),), |cx, value| {
                        cx.text(format!("Nested {}", *value.read()))
                    });
                });
            }
            cx.text("Region footer");
        });
        runtime.flush();
        Self {
            runtime,
            component,
            root,
            visible,
            title,
            value,
        }
    }
    fn child(&self) -> RegionId {
        self.runtime.child(self.root, 10).unwrap()
    }
    fn grand(&self) -> RegionId {
        self.runtime.child(self.child(), 20).unwrap()
    }
}
fn compare_nodes(a: &Engine, ai: Entity, b: &Engine, bi: Entity) {
    let a_node = a.node(ai).unwrap();
    let b_node = b.node(bi).unwrap();
    assert_eq!(a_node.kind, b_node.kind);
    assert_eq!(a_node.text, b_node.text);
    assert_eq!(a_node.style, b_node.style);
    assert_eq!(a_node.children.len(), b_node.children.len());
    for (&ac, &bc) in a_node.children.iter().zip(b_node.children) {
        compare_nodes(a, ac, b, bc);
    }
}
fn equivalent(a: &mut EngineBridge, b: &mut EngineBridge, text: &mut TextContext) {
    compare_nodes(
        &a.source.engine,
        a.source.engine.root(),
        &b.source.engine,
        b.source.engine.root(),
    );
    fn count(engine: &Engine, id: Entity) -> usize {
        1 + engine
            .node(id)
            .unwrap()
            .children
            .iter()
            .map(|&id| count(engine, id))
            .sum::<usize>()
    }
    let n = count(&a.source.engine, a.source.engine.root());
    assert_eq!(a.source.experiment_live_counts(), (n, n, n, n));
    assert_eq!(b.source.experiment_live_counts(), (n, n, n, n));
    let ap = &a.source.adapter.placed;
    let bp = &b.source.adapter.placed;
    assert_eq!(ap.len(), bp.len());
    for (a, b) in ap.iter().zip(bp) {
        assert_eq!(a.depth, b.depth);
        assert_eq!(a.rect, b.rect);
        assert_eq!(a.content_origin, b.content_origin);
        assert_eq!(a.content_size, b.content_size);
        assert_eq!(a.clip, b.clip);
    }
    a.encode(text);
    b.encode(text);
    assert!(
        a.scene.commands == b.scene.commands,
        "encoded scenes differ"
    );
}
#[test]
fn child_replay_preserves_parent_siblings_inherited_style_and_unvisited_descendants() {
    let mut f = Fixture::new();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&f.runtime, f.root, &mut text);
    let mut b = EngineBridge::new(&f.runtime, f.root, &mut text);
    let root = a.entity(f.root).unwrap();
    let child = a.entity(f.child()).unwrap();
    let grand = a.entity(f.grand()).unwrap();
    let shell = a
        .source
        .engine
        .find(a.source.engine.root(), "shell")
        .unwrap();
    let before = a.source.engine.find(shell, "before").unwrap();
    let after = a.source.engine.find(shell, "after").unwrap();
    f.runtime
        .access(f.value)
        .update(|v| *v = "a much longer updated paragraph ".repeat(12));
    a.apply(&mut f.runtime, &mut text);
    b.rebuild(&f.runtime, &mut text);
    equivalent(&mut a, &mut b, &mut text);
    assert_eq!(a.entity(f.root), Some(root));
    assert_eq!(a.entity(f.child()), Some(child));
    assert_eq!(a.entity(f.grand()), Some(grand));
    assert_eq!(a.source.engine.find(shell, "before"), Some(before));
    assert_eq!(a.source.engine.find(shell, "after"), Some(after));
    assert_eq!(a.last_passes.len(), 2);
    assert_eq!(
        a.last_passes.iter().map(|(_, s)| s.visited).sum::<usize>(),
        3
    );
    assert!(b.source.engine.last_stats().visited > 3);
    let leaf = a.source.engine.node(child).unwrap().children[0];
    assert_eq!(a.source.engine.node(leaf).unwrap().style.font_size, 19);
    assert_eq!(
        a.source.engine.node(leaf).unwrap().style.foreground,
        plico::Color::Literal(0x254059)
    );
}
#[test]
fn branch_removal_remount_and_component_disposal_match_full_pipeline() {
    let mut f = Fixture::new();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&f.runtime, f.root, &mut text);
    let mut b = EngineBridge::new(&f.runtime, f.root, &mut text);
    let child_id = f.child();
    let old_entity = a.entity(child_id).unwrap();
    let stale = a.context(child_id).unwrap();
    let event = f
        .runtime
        .on_click(child_id, (f.value,), |v| v.update(|v| v.push('!')));
    assert!(f.runtime.click(event));
    f.runtime.access(f.visible).update(|v| *v = false);
    a.apply(&mut f.runtime, &mut text);
    b.rebuild(&f.runtime, &mut text);
    equivalent(&mut a, &mut b, &mut text);
    assert!(a.source.engine.node(old_entity).is_none());
    assert!(!f.runtime.click(event));
    assert!(f.runtime.state_alive(f.value));
    assert_eq!(a.bindings_len(), 1);
    assert!(matches!(
        a.source.experiment_redeclare(stale, |_| {}),
        Err(FrameError::InvalidRegion)
    ));
    f.runtime.access(f.visible).update(|v| *v = true);
    a.apply(&mut f.runtime, &mut text);
    b.rebuild(&f.runtime, &mut text);
    equivalent(&mut a, &mut b, &mut text);
    assert_ne!(a.entity(f.child()), Some(old_entity));
    assert_eq!(&*f.runtime.access(f.value).read(), "short!");
    f.runtime.unmount_component(f.component);
    a.apply(&mut f.runtime, &mut text);
    b.rebuild(&f.runtime, &mut text);
    equivalent(&mut a, &mut b, &mut text);
    assert_eq!(a.bindings_len(), 0);
    assert_eq!(f.runtime.live_counts(), (0, 0, 0, 0, 0));
}
#[test]
fn parent_and_children_replay_in_one_backend_commit_matches_reference() {
    let mut f = Fixture::new();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&f.runtime, f.root, &mut text);
    let mut b = EngineBridge::new(&f.runtime, f.root, &mut text);
    f.runtime.access(f.value).update(|v| *v = "changed".into());
    f.runtime.access(f.title).update(|v| *v = true);
    a.apply(&mut f.runtime, &mut text);
    b.rebuild(&f.runtime, &mut text);
    equivalent(&mut a, &mut b, &mut text);
    assert_eq!(a.last_passes.len(), 3);
    assert_eq!(a.last_passes[0].0, f.root);
}
#[test]
fn keyed_region_reorder_insert_remove_updates_real_layout_without_replacing_survivors() {
    let mut runtime = Runtime::default();
    let owner = runtime.component();
    let order = runtime.state_in(owner, 1, || vec![10_u64, 20]);
    for key in [10, 20, 30] {
        runtime.state_in(owner, key, || key as i32);
    }
    let root = runtime.watch_in(owner, (order,), |cx, order| {
        for &key in order.read().iter() {
            cx.watch(key, (cx.state::<i32>(key),), |cx, v| {
                cx.text(v.read().to_string())
            });
        }
    });
    runtime.flush();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&runtime, root, &mut text);
    let mut b = EngineBridge::new(&runtime, root, &mut text);
    let first = runtime.child(root, 10).unwrap();
    let first_entity = a.entity(first).unwrap();
    for next in [
        vec![20, 10],
        vec![20, 30, 10],
        vec![30, 10],
        vec![],
        vec![10, 30],
    ] {
        runtime.access(order).update(|v| *v = next);
        a.apply(&mut runtime, &mut text);
        b.rebuild(&runtime, &mut text);
        equivalent(&mut a, &mut b, &mut text);
        if let Some(still_live) = runtime.child(root, 10)
            && still_live == first
        {
            assert_eq!(a.entity(first), Some(first_entity));
        }
    }
}
#[test]
fn deterministic_mixed_updates_match_full_tree_layout_and_scene_each_step() {
    let mut f = Fixture::new();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&f.runtime, f.root, &mut text);
    let mut b = EngineBridge::new(&f.runtime, f.root, &mut text);
    let mut seed = 0x12345678_u32;
    for round in 0..120 {
        for _ in 0..3 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            match seed % 3 {
                0 => f.runtime.access(f.visible).update(|v| *v = !*v),
                1 => f.runtime.access(f.title).update(|v| *v = !*v),
                _ => f.runtime.access(f.value).update(|v| {
                    *v = format!("round {round} {}", "text ".repeat((seed % 30) as usize))
                }),
            }
        }
        a.apply(&mut f.runtime, &mut text);
        b.rebuild(&f.runtime, &mut text);
        equivalent(&mut a, &mut b, &mut text);
    }
}

#[test]
fn global_inherited_style_refreshes_contexts_before_next_local_replay() {
    let mut f = Fixture::new();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&f.runtime, f.root, &mut text);
    let mut b = EngineBridge::new(&f.runtime, f.root, &mut text);
    let child = a.entity(f.child()).unwrap();
    let stale = a.context(f.child()).unwrap();
    let color = plico::Color::Literal(0x995522);
    a.set_shell_style(&f.runtime, 25, color, &mut text);
    b.set_shell_style(&f.runtime, 25, color, &mut text);
    assert_eq!(a.entity(f.child()), Some(child));
    assert!(matches!(
        a.source.experiment_redeclare(stale, |_| {}),
        Err(FrameError::InvalidRegion)
    ));
    f.runtime
        .access(f.value)
        .update(|v| *v = "new style".into());
    a.apply(&mut f.runtime, &mut text);
    b.rebuild(&f.runtime, &mut text);
    equivalent(&mut a, &mut b, &mut text);
    let leaf = a.source.engine.node(child).unwrap().children[0];
    assert_eq!(a.source.engine.node(leaf).unwrap().style.font_size, 25);
    assert_eq!(a.source.engine.node(leaf).unwrap().style.foreground, color);
}

#[test]
fn host_full_rebuild_uses_latest_incremental_descriptions_without_graph_snapshot() {
    let mut f = Fixture::new();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&f.runtime, f.root, &mut text);
    let mut b = EngineBridge::new(&f.runtime, f.root, &mut text);
    for visible in [true, false, true] {
        f.runtime.access(f.value).update(|v| v.push_str(" updated"));
        f.runtime.access(f.visible).update(|v| *v = visible);
        a.apply(&mut f.runtime, &mut text);
        b.rebuild(&f.runtime, &mut text);
        a.force_host_rebuild(&mut text);
        equivalent(&mut a, &mut b, &mut text);
    }
}
#[test]
fn empty_new_child_still_has_a_description_for_host_rebuild() {
    let mut runtime = Runtime::default();
    let visible = runtime.state(|| false);
    let root = runtime.watch((visible,), |cx, visible| {
        if *visible.read() {
            cx.watch(1, (visible.handle(),), |_, _| {});
        }
    });
    runtime.flush();
    let mut text = TextContext::new();
    let mut a = EngineBridge::new(&runtime, root, &mut text);
    let mut b = EngineBridge::new(&runtime, root, &mut text);
    runtime.access(visible).update(|v| *v = true);
    a.apply(&mut runtime, &mut text);
    b.rebuild(&runtime, &mut text);
    a.force_host_rebuild(&mut text);
    equivalent(&mut a, &mut b, &mut text);
}

#[test]
fn retained_children_cannot_be_mixed_with_new_child_declarations() {
    let mut f = Fixture::new();
    let mut text = TextContext::new();
    let mut bridge = EngineBridge::new(&f.runtime, f.root, &mut text);
    let context = bridge.context(f.child()).unwrap();
    let result = bridge.source.experiment_redeclare(context, |cx| {
        cx.experiment_keep_children();
        cx.text("illegal additional child");
    });
    assert!(matches!(result, Err(FrameError::InvalidRegion)));
    // Errors require a full recovery pass, as documented for this protocol.
    f.runtime
        .access(f.value)
        .update(|v| *v = "recovered".into());
    f.runtime.flush();
    bridge.rebuild(&f.runtime, &mut text);
    let mut reference = EngineBridge::new(&f.runtime, f.root, &mut text);
    equivalent(&mut bridge, &mut reference, &mut text);
}
