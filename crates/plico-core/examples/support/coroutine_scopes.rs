//! An owned forest of isolated Engines, not shared-tree partial submission.
use super::*;
use plico_core::{Arena, Entity};

#[path = "coroutine_composite.rs"]
mod composite;

#[path = "coroutine_comparison.rs"]
mod comparison;
pub(super) fn compare() {
    comparison::run();
}

pub(super) fn benchmark() {
    composite::benchmark();
}

type Task = Pin<Box<dyn Future<Output = ()>>>;

// Same identity ingredients as component-generated widget keys. Written by hand
// until lifecycle semantics are established; no new macro or syntax is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ChildIdentity {
    site: u64,
    business_key: Option<u64>,
}
struct ChildSpec {
    identity: ChildIdentity,
    make: Box<dyn FnOnce(AsyncUi) -> Task>,
}
impl ChildSpec {
    fn counter(site: u64, business_key: Option<u64>) -> Self {
        Self {
            identity: ChildIdentity { site, business_key },
            make: Box::new(|ui| Box::pin(counter(ui, Arc::default()))),
        }
    }
}
// Explicit parent-to-child input, not tracking arbitrary local variable reads.
#[derive(Clone)]
struct Props<T> {
    value: Rc<RefCell<T>>,
    wake: Arc<WakeFlag>,
}
impl<T: Clone + PartialEq> Props<T> {
    fn new(ui: &AsyncUi, value: T) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
            wake: ui.wake.clone(),
        }
    }
    fn get(&self) -> T {
        self.value.borrow().clone()
    }
    fn set(&self, value: T) {
        let mut current = self.value.borrow_mut();
        if *current != value {
            *current = value;
            self.wake.wake_by_ref();
        }
    }
}

async fn titled_counter(ui: AsyncUi, title: Props<String>) {
    let mut count = 0;
    loop {
        ui.frame(|cx| {
            // Read props when the closure actually runs, not before awaiting it.
            text(cx, title.get());
            text!(cx, "Count: {count}");
            if button(cx).text("Increment").key("increment").clicked() {
                count += 1;
                cx.request_rebuild();
            }
        })
        .await;
    }
}

fn props_demo() {
    let mut scopes = Scopes::default();
    let root = scopes.mount(None, |ui| Box::pin(panel(ui, "root", Rc::default())));
    let slot = Rc::new(RefCell::new(None));
    let sender = slot.clone();
    let spec = ChildSpec {
        identity: ChildIdentity {
            site: 1,
            business_key: None,
        },
        make: Box::new(move |ui| {
            let title = Props::new(&ui, "Old title".to_owned());
            *sender.borrow_mut() = Some(title.clone());
            Box::pin(titled_counter(ui, title))
        }),
    };
    let child = scopes.reconcile(root, vec![spec]).unwrap()[0];
    scopes.pump();
    scopes.click(child, "increment");
    let root_declarations = scopes.declarations(root);
    let props = slot.borrow().as_ref().unwrap().clone();
    props.set("Intermediate".into());
    props.set("New title".into());
    assert_eq!(scopes.pump(), 1); // Latest value wins; notifications coalesce.
    assert!(scopes.has_text(child, "New title"));
    assert!(scopes.has_text(child, "Count: 1"));
    assert_eq!(scopes.declarations(root), root_declarations);
    props.set("New title".into());
    assert_eq!(scopes.pump(), 0);
    scopes.remove(child);
    props.set("Late update".into());
    assert_eq!(scopes.pump(), 0);
    println!("props: explicit input updates preserve local state and coalesce wakes OK");
}

struct Scope {
    identity: Option<ChildIdentity>,
    parent: Option<Entity>,
    children: Vec<Entity>,
    host: Host,
    polls: usize,
}
#[derive(Default)]
struct Scopes {
    nodes: Arena<Scope>,
}
impl Scopes {
    fn mount(&mut self, parent: Option<Entity>, make: impl FnOnce(AsyncUi) -> Task) -> Entity {
        if let Some(parent) = parent {
            assert!(self.nodes.get(parent).is_some());
        }
        let host = Host::new(make);
        let id = self.nodes.insert(Scope {
            identity: None,
            parent,
            children: Vec::new(),
            host,
            polls: 0,
        });
        if let Some(parent) = parent {
            self.nodes.get_mut(parent).unwrap().children.push(id);
        }
        id
    }

    fn reconcile(
        &mut self,
        parent: Entity,
        children: Vec<ChildSpec>,
    ) -> Result<Vec<Entity>, &'static str> {
        let parent_scope = self.nodes.get(parent).ok_or("parent was removed")?;
        let mut seen = std::collections::HashSet::new();
        for child in &children {
            if !seen.insert(child.identity) {
                return Err("duplicate child identity");
            }
        }
        let old = parent_scope.children.clone();
        let mut existing = HashMap::new();
        for &id in &old {
            let identity = self
                .nodes
                .get(id)
                .unwrap()
                .identity
                .ok_or("unmanaged child")?;
            existing.insert(identity, id);
        }
        // Factories run only for new identities. Reordering or re-declaring a
        // parent must not recreate a Future and reset its local variables.
        let mut ordered = Vec::with_capacity(children.len());
        for child in children {
            let id = if let Some(id) = existing.remove(&child.identity) {
                id
            } else {
                let id = self.mount(Some(parent), child.make);
                self.nodes.get_mut(id).unwrap().identity = Some(child.identity);
                id
            };
            ordered.push(id);
        }
        // Removal follows previous sibling order, with descendant-first cleanup.
        for id in old {
            if self
                .nodes
                .get(id)
                .is_some_and(|s| existing.contains_key(&s.identity.unwrap()))
            {
                self.remove(id);
            }
        }
        self.nodes
            .get_mut(parent)
            .unwrap()
            .children
            .clone_from(&ordered);
        Ok(ordered)
    }

    fn pump(&mut self) -> usize {
        // Deliberately simple O(scope count) scan. Only woken Futures are polled;
        // this is not an O(delta) runnable queue or a renderer scheduling claim.
        let ids: Vec<_> = self.nodes.iter().map(|(id, _)| id).collect();
        let mut polls = 0;
        for id in ids {
            if let Some(scope) = self.nodes.get_mut(id) {
                let count = scope.host.pump();
                scope.polls += count;
                polls += count;
                if scope.host.task.is_none() {
                    self.remove(id);
                }
            }
        }
        polls
    }

    fn click(&mut self, id: Entity, key: &'static str) -> bool {
        let Some(scope) = self.nodes.get_mut(id) else {
            return false;
        };
        scope.host.click(key);
        true
    }

    fn remove(&mut self, root: Entity) {
        let Some(scope) = self.nodes.get(root) else {
            return;
        };
        let parent = scope.parent;
        // Traverse ownership edges, never remove random arena entries.
        let mut stack = vec![(root, false)];
        while let Some((id, visited)) = stack.pop() {
            if visited {
                self.nodes.remove(id); // Host::drop cancels task and clears nodes.
            } else if let Some(scope) = self.nodes.get(id) {
                stack.push((id, true));
                stack.extend(scope.children.iter().rev().map(|&child| (child, false)));
            }
        }
        if let Some(parent) = parent.and_then(|id| self.nodes.get_mut(id)) {
            parent.children.retain(|&id| id != root);
        }
    }

    fn declarations(&self, id: Entity) -> usize {
        self.nodes
            .get(id)
            .unwrap()
            .host
            .ui
            .session
            .borrow()
            .declarations
    }
    fn has_text(&self, id: Entity, text: &str) -> bool {
        self.nodes.get(id).unwrap().host.has_text(text)
    }
}
impl Drop for Scopes {
    fn drop(&mut self) {
        // Keep child-before-parent cancellation even when dropping the scheduler.
        loop {
            let root = self
                .nodes
                .iter()
                .find_map(|(id, s)| s.parent.is_none().then_some(id));
            let Some(root) = root else {
                break;
            };
            self.remove(root);
        }
    }
}

async fn panel(ui: AsyncUi, label: &'static str, log: Rc<RefCell<Vec<&'static str>>>) {
    struct Guard(&'static str, Rc<RefCell<Vec<&'static str>>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.1.borrow_mut().push(self.0);
        }
    }
    let _guard = Guard(label, log);
    loop {
        ui.frame(|cx| {
            text(cx, label);
        })
        .await;
    }
}

pub(super) fn run() {
    let mut scopes = Scopes::default();
    let log = Rc::new(RefCell::new(Vec::new()));
    let root = scopes.mount(None, |ui| Box::pin(panel(ui, "workspace", log.clone())));
    let left = scopes.mount(Some(root), |ui| Box::pin(counter(ui, Arc::default())));
    let completion = Completion::default();
    let right = scopes.mount(Some(root), |ui| Box::pin(workflow(ui, completion.clone())));
    assert_eq!(scopes.pump(), 3);
    let left_initial = scopes.declarations(left);
    let root_initial = scopes.declarations(root);
    scopes.click(right, "confirm");
    assert!(scopes.has_text(right, "Deleting..."));
    assert!(scopes.has_text(left, "Count: 0"));
    assert_eq!(scopes.declarations(left), left_initial);
    scopes.click(left, "increment");
    assert!(scopes.has_text(left, "Count: 1"));
    assert!(scopes.has_text(right, "Deleting..."));
    let left_after_click = scopes.declarations(left);
    assert_eq!(scopes.pump(), 0);
    completion.complete(42);
    assert_eq!(scopes.pump(), 1);
    assert!(scopes.has_text(right, "Deleted: 42"));
    assert_eq!(scopes.declarations(left), left_after_click);
    assert_eq!(scopes.declarations(root), root_initial);
    scopes.remove(root);
    assert!(scopes.nodes.is_empty());
    assert!(!scopes.click(left, "increment"));
    assert_eq!(scopes.pump(), 0);
    assert_eq!(*log.borrow(), ["workspace"]);
    println!("scopes: independent counter/network wake, retained siblings, parent cancellation OK");
    identity_demo();
    props_demo();
    composite::run();
}

fn identity_demo() {
    const SITE: u64 = plico_core::macro_support::site_id("scope.list", 0);
    let mut scopes = Scopes::default();
    let root = scopes.mount(None, |ui| Box::pin(panel(ui, "list", Rc::default())));
    let items = scopes
        .reconcile(
            root,
            vec![
                ChildSpec::counter(SITE, Some(10)),
                ChildSpec::counter(SITE, Some(20)),
            ],
        )
        .unwrap();
    scopes.pump();
    scopes.click(items[0], "increment");
    let declarations = scopes.declarations(items[0]);
    let reordered = scopes
        .reconcile(
            root,
            vec![
                ChildSpec::counter(SITE, Some(20)),
                ChildSpec::counter(SITE, Some(10)),
            ],
        )
        .unwrap();
    assert_eq!(reordered, [items[1], items[0]]);
    assert_eq!(scopes.pump(), 0);
    assert_eq!(scopes.declarations(items[0]), declarations);
    assert!(scopes.has_text(items[0], "Count: 1"));
    scopes
        .reconcile(root, vec![ChildSpec::counter(SITE, Some(20))])
        .unwrap();
    assert!(scopes.nodes.get(items[0]).is_none());
    let remounted = scopes
        .reconcile(root, vec![ChildSpec::counter(SITE, Some(10))])
        .unwrap()[0];
    assert_ne!(remounted, items[0]);
    scopes.pump();
    assert!(scopes.has_text(remounted, "Count: 0"));
    println!("identity: keyed reorder preserves Futures, unmount/remount resets locals OK");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_scopes_keep_siblings_while_waiting_and_waking() {
        run();
    }

    #[test]
    fn parent_exit_drops_children_first_and_stale_completion_cannot_target_replacement() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut scopes = Scopes::default();
        let root = scopes.mount(None, |ui| Box::pin(panel(ui, "parent", log.clone())));
        let child = scopes.mount(Some(root), |ui| Box::pin(panel(ui, "child", log.clone())));
        let _grandchild = scopes.mount(Some(child), |ui| {
            Box::pin(panel(ui, "grandchild", log.clone()))
        });
        let completion = Completion::default();
        let waiting = scopes.mount(Some(root), |ui| Box::pin(workflow(ui, completion.clone())));
        scopes.pump();
        scopes.click(waiting, "confirm");
        scopes.remove(root);
        assert_eq!(*log.borrow(), ["grandchild", "child", "parent"]);
        let replacement = scopes.mount(None, |ui| Box::pin(panel(ui, "replacement", log.clone())));
        scopes.pump();
        assert_ne!(waiting, replacement);
        assert!(!scopes.click(waiting, "confirm"));
        let declarations = scopes.declarations(replacement);
        completion.complete(9);
        assert_eq!(scopes.pump(), 0);
        assert_eq!(scopes.declarations(replacement), declarations);
        assert!(scopes.has_text(replacement, "replacement"));
    }

    #[test]
    fn identical_state_keys_are_local_and_one_reload_does_not_redeclare_siblings() {
        let mut scopes = Scopes::default();
        let left = scopes.mount(None, |ui| Box::pin(counter(ui, Arc::default())));
        let right = scopes.mount(None, |ui| Box::pin(counter(ui, Arc::default())));
        scopes.pump();
        scopes.click(left, "increment");
        assert!(scopes.has_text(left, "Count: 1"));
        assert!(scopes.has_text(right, "Count: 0"));
        let right_declarations = scopes.declarations(right);
        scopes
            .nodes
            .get_mut(left)
            .unwrap()
            .host
            .reload(|ui| Box::pin(counter(ui, Arc::default())));
        assert!(scopes.has_text(left, "Count: 1"));
        assert_eq!(scopes.declarations(right), right_declarations);
        scopes.remove(left);
        assert!(scopes.has_text(right, "Count: 0"));
        assert_eq!(scopes.pump(), 0);
    }

    #[test]
    fn call_sites_and_business_keys_preserve_instances() {
        identity_demo();
        props_demo();
    }

    #[test]
    fn static_sites_parent_namespaces_and_duplicate_identity_validation() {
        let mut scopes = Scopes::default();
        let left = scopes.mount(None, |ui| Box::pin(panel(ui, "left", Rc::default())));
        let right = scopes.mount(None, |ui| Box::pin(panel(ui, "right", Rc::default())));
        let a = scopes
            .reconcile(
                left,
                vec![ChildSpec::counter(1, None), ChildSpec::counter(2, None)],
            )
            .unwrap();
        let b = scopes
            .reconcile(right, vec![ChildSpec::counter(1, None)])
            .unwrap();
        scopes.pump();
        assert_ne!(a[0], b[0]);
        scopes.click(a[0], "increment");
        assert!(scopes.has_text(b[0], "Count: 0"));
        let length = scopes.nodes.len();
        assert_eq!(
            scopes
                .reconcile(
                    left,
                    vec![ChildSpec::counter(1, None), ChildSpec::counter(1, None)]
                )
                .unwrap_err(),
            "duplicate child identity"
        );
        assert_eq!(scopes.nodes.len(), length);
        assert_eq!(scopes.nodes.get(left).unwrap().children, a);
        assert!(scopes.has_text(a[0], "Count: 1"));
        let without_first = scopes
            .reconcile(left, vec![ChildSpec::counter(2, None)])
            .unwrap();
        assert_eq!(without_first, [a[1]]); // Conditional sibling does not shift identity.
    }

    #[test]
    fn retained_factory_does_not_implicitly_update_captured_props() {
        fn spec(label: &'static str) -> ChildSpec {
            ChildSpec {
                identity: ChildIdentity {
                    site: 1,
                    business_key: None,
                },
                make: Box::new(move |ui| Box::pin(panel(ui, label, Rc::default()))),
            }
        }
        let mut scopes = Scopes::default();
        let root = scopes.mount(None, |ui| Box::pin(panel(ui, "root", Rc::default())));
        let id = scopes.reconcile(root, vec![spec("old prop")]).unwrap()[0];
        scopes.pump();
        assert_eq!(
            scopes.reconcile(root, vec![spec("new prop")]).unwrap(),
            [id]
        );
        assert_eq!(scopes.pump(), 0);
        assert!(scopes.has_text(id, "old prop"));
        // Reusing a Future retains its original captures. A real component API
        // requires explicit prop delivery + wake, not just a stable key.
    }

    #[test]
    fn props_updates_preserve_locals_coalesce_and_stop_after_unmount() {
        props_demo();
    }

    #[test]
    fn finishing_parent_cancels_existing_children() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut scopes = Scopes::default();
        let completion = Completion::default();
        let task = completion.clone();
        let root = scopes.mount(None, |_ui| {
            Box::pin(async move {
                task.await;
            })
        });
        scopes.mount(Some(root), |ui| Box::pin(panel(ui, "child", log.clone())));
        scopes.pump();
        completion.complete(1);
        scopes.pump();
        assert!(scopes.nodes.is_empty());
        assert_eq!(*log.borrow(), ["child"]);
    }
}
