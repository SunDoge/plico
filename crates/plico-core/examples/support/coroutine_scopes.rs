//! An owned forest of isolated Engines, not shared-tree partial submission.
use super::*;
use plico_core::{Arena, Entity};

type Task = Pin<Box<dyn Future<Output = ()>>>;
struct Scope {
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
