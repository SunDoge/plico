//! Compose retained scope declarations into ONE Engine through public replay API.
//! This skips idle business Futures, but still replays/scans the entire tree.
use super::*;
use plico_core::{FrameStats, Key, Kind};

#[derive(Default)]
struct Composite {
    engine: Engine,
    routes: HashMap<Entity, (Entity, Entity)>, // global -> (scope, local node)
}
impl Composite {
    fn commit(&mut self, scopes: &Scopes) -> FrameStats {
        fn nodes(
            cx: &mut UiContext<'_>,
            local: &Engine,
            parent: Entity,
            scope: Entity,
            clicks: &mut Vec<(Entity, Entity)>,
        ) {
            for &id in local.node(parent).unwrap().children {
                let node = local.node(id).unwrap();
                // Fail explicitly rather than imply full renderer/portal parity.
                assert!(matches!(
                    node.kind,
                    Kind::Column
                        | Kind::Row
                        | Kind::Text
                        | Kind::Button
                        | Kind::ButtonBase
                        | Kind::Checkbox
                        | Kind::Switch
                        | Kind::TextInput
                ));
                assert!(node.shader.is_none() && node.numeric.is_none() && node.spans.is_empty());
                let (_, clicked) = cx.element_with_grid(
                    Key::Number(id.raw()),
                    node.kind,
                    node.style,
                    node.text,
                    node.grid,
                    |cx| {
                        if let Some((buffer, placeholder)) = local.input(id) {
                            cx.bind_input(buffer, placeholder);
                        }
                        nodes(cx, local, id, scope, clicks);
                    },
                );
                if clicked {
                    clicks.push((scope, id));
                }
            }
        }
        fn scope(
            cx: &mut UiContext<'_>,
            scopes: &Scopes,
            id: Entity,
            clicks: &mut Vec<(Entity, Entity)>,
        ) {
            let s = scopes.nodes.get(id).unwrap();
            let _ = column(cx).key(id.raw()).gap(4.)
                + |cx| {
                    let _ = column(cx).key("content")
                        + |cx| {
                            let local = s.host.ui.session.borrow();
                            nodes(cx, &local.engine, local.engine.root(), id, clicks);
                        };
                    for &child in &s.children {
                        scope(cx, scopes, child, clicks);
                    }
                };
        }
        let mut clicks = Vec::new();
        let stats = self
            .engine
            .frame(|cx| {
                let _ = column(cx).key("forest").gap(10.)
                    + |cx| {
                        for (id, s) in scopes.nodes.iter() {
                            if s.parent.is_none() {
                                scope(cx, scopes, id, &mut clicks);
                            }
                        }
                    };
            })
            .unwrap();
        // Replay consumed global clicks; enqueue in the owning local Engine.
        // Futures are polled only after all shared-tree borrows have ended.
        for (scope, id) in clicks {
            let s = scopes.nodes.get(scope).unwrap();
            s.host.ui.session.borrow_mut().engine.queue_click(id);
            s.host.ui.invalidate();
        }
        self.routes.clear();
        fn map_nodes(
            global: &Engine,
            global_parent: Entity,
            local: &Engine,
            parent: Entity,
            scope: Entity,
            routes: &mut HashMap<Entity, (Entity, Entity)>,
        ) {
            for &id in local.node(parent).unwrap().children {
                let global_id = global.find(global_parent, id.raw()).unwrap();
                routes.insert(global_id, (scope, id));
                map_nodes(global, global_id, local, id, scope, routes);
            }
        }
        fn map_scope(
            global: &Engine,
            parent: Entity,
            scopes: &Scopes,
            id: Entity,
            routes: &mut HashMap<Entity, (Entity, Entity)>,
        ) {
            let wrapper = global.find(parent, id.raw()).unwrap();
            let content = global.find(wrapper, "content").unwrap();
            let s = scopes.nodes.get(id).unwrap();
            let local = s.host.ui.session.borrow();
            map_nodes(
                global,
                content,
                &local.engine,
                local.engine.root(),
                id,
                routes,
            );
            for &child in &s.children {
                map_scope(global, wrapper, scopes, child, routes);
            }
        }
        let forest = self.engine.find(self.engine.root(), "forest").unwrap();
        for (id, s) in scopes.nodes.iter() {
            if s.parent.is_none() {
                map_scope(&self.engine, forest, scopes, id, &mut self.routes);
            }
        }
        stats
    }

    fn button(&self, scope: Entity, label: &str) -> Entity {
        self.routes
            .iter()
            .find_map(|(&global, &(owner, _))| {
                let node = self.engine.node(global).unwrap();
                (owner == scope
                    && matches!(node.kind, Kind::Button | Kind::ButtonBase)
                    && self.engine.accessible_name(global).as_deref() == Some(label))
                .then_some(global)
            })
            .expect("button in shared tree")
    }

    fn click(&mut self, scopes: &mut Scopes, global: Entity) -> bool {
        let Some(&(owner, local_id)) = self.routes.get(&global) else {
            return false;
        };
        // Reject stale shared output before replaying an already removed scope.
        if !scopes
            .nodes
            .get(owner)
            .is_some_and(|s| s.host.ui.session.borrow().engine.node(local_id).is_some())
        {
            return false;
        }
        self.engine.queue_click(global);
        self.commit(scopes);
        scopes.pump();
        self.commit(scopes);
        true
    }
}

pub(super) fn run() {
    let mut scopes = Scopes::default();
    let parent = scopes.mount(None, |ui| Box::pin(panel(ui, "Workspace", Rc::default())));
    let children = scopes
        .reconcile(
            parent,
            vec![
                ChildSpec::counter(1, Some(10)),
                ChildSpec::counter(1, Some(20)),
            ],
        )
        .unwrap();
    scopes.pump();
    let mut shared = Composite::default();
    shared.commit(&scopes);
    let left_button = shared.button(children[0], "Increment");
    let right_button = shared.button(children[1], "Increment");
    assert_ne!(left_button, right_button);
    let right_declarations = scopes.declarations(children[1]);
    assert!(shared.click(&mut scopes, left_button));
    assert!(scopes.has_text(children[0], "Count: 1"));
    assert!(scopes.has_text(children[1], "Count: 0"));
    assert_eq!(scopes.declarations(children[1]), right_declarations);
    assert_eq!(shared.button(children[1], "Increment"), right_button);
    let stable = shared.commit(&scopes);
    assert_eq!(stable.created, 0);
    assert_eq!(stable.order_changes, 0);
    assert_eq!(stable.layout_runs, 0);
    // Reorder one parent's children: global IDs and local state survive.
    scopes
        .reconcile(
            parent,
            vec![
                ChildSpec::counter(1, Some(20)),
                ChildSpec::counter(1, Some(10)),
            ],
        )
        .unwrap();
    let reordered = shared.commit(&scopes);
    assert!(reordered.order_changes > 0);
    assert_eq!(shared.button(children[0], "Increment"), left_button);
    scopes.remove(children[0]);
    assert!(!shared.click(&mut scopes, left_button));
    shared.commit(&scopes);
    assert!(shared.engine.node(left_button).is_none());
    assert!(shared.click(&mut scopes, right_button));
    assert!(scopes.has_text(children[1], "Count: 1"));
    println!(
        "composite: one Engine, scoped global input, retained IDs, reorder/removal OK (full replay)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_tree_routes_duplicate_buttons_and_preserves_reordered_ids() {
        run();
    }

    #[test]
    fn shared_input_binding_updates_local_form_without_serializing_buffer() {
        let mut scopes = Scopes::default();
        let form = scopes.mount(None, |ui| Box::pin(settings(ui)));
        scopes.pump();
        let mut shared = Composite::default();
        shared.commit(&scopes);
        let input = shared
            .routes
            .keys()
            .copied()
            .find(|&id| shared.engine.node(id).unwrap().kind == Kind::TextInput)
            .unwrap();
        let buffer = shared.engine.input(input).unwrap().0.clone();
        buffer.set("Shared editor".to_owned().into());
        scopes.nodes.get(form).unwrap().host.ui.invalidate();
        scopes.pump();
        let save = shared.button(form, "Save");
        assert!(shared.click(&mut scopes, save));
        assert!(scopes.has_text(form, "Saved: Shared editor"));
        scopes.remove(form);
        shared.commit(&scopes);
        assert!(shared.engine.input(input).is_none());
        assert!(!shared.click(&mut scopes, save));
    }

    #[test]
    fn sparse_business_execution_still_requires_full_shared_replay() {
        let mut scopes = Scopes::default();
        let ids: Vec<_> = (0..100)
            .map(|_| scopes.mount(None, |ui| Box::pin(counter(ui, Arc::default()))))
            .collect();
        scopes.pump();
        let mut shared = Composite::default();
        shared.commit(&scopes);
        let before: Vec<_> = ids.iter().map(|&id| scopes.declarations(id)).collect();
        let target = 42;
        let button = shared.button(ids[target], "Increment");
        shared.click(&mut scopes, button);
        for (index, &id) in ids.iter().enumerate() {
            assert_eq!(
                scopes.declarations(id) - before[index],
                if index == target { 2 } else { 0 }
            );
        }
        let stats = shared.commit(&scopes);
        assert!(stats.visited >= 500); // Shared reconciliation still visits idle scopes.
        assert_eq!(stats.layout_runs, 0);
        println!(
            "100 scopes: one counter executes 2 passes, 99 skip business code; shared replay visits {} nodes",
            stats.visited
        );
    }

    #[test]
    fn child_height_relayouts_shared_ancestors_without_polling_sibling() {
        let mut scopes = Scopes::default();
        let input = Rc::new(RefCell::new(None));
        let sender = input.clone();
        let changing = scopes.mount(None, move |ui| {
            let height = Props::new(&ui, 30_i32);
            *sender.borrow_mut() = Some(height.clone());
            Box::pin(async move {
                loop {
                    ui.frame(|cx| {
                        let _ = column(cx).height(height.get() as f32) + |_cx| {};
                    })
                    .await;
                }
            })
        });
        let sibling = scopes.mount(None, |ui| Box::pin(counter(ui, Arc::default())));
        scopes.pump();
        let mut shared = Composite::default();
        shared.commit(&scopes);
        let before = shared.engine.node(shared.engine.root()).unwrap().size;
        let sibling_declarations = scopes.declarations(sibling);
        input.borrow().as_ref().unwrap().set(100);
        assert_eq!(scopes.pump(), 1);
        let changed = shared.commit(&scopes);
        let after = shared.engine.node(shared.engine.root()).unwrap().size;
        assert_eq!(after.height - before.height, 70.);
        assert!(changed.layout_runs > 0);
        assert_eq!(scopes.declarations(sibling), sibling_declarations);
        assert!(scopes.nodes.get(changing).is_some());
        assert_eq!(shared.commit(&scopes).layout_runs, 0);
    }
}
