use plico_reactivity_compare::{ComponentId, HandlerId, RegionId, Runtime, State};
use std::{cell::Cell, rc::Rc};

const COUNT: u64 = 10;
const PROBE: u64 = 11;
const CHILD: u64 = 20;
const GRANDCHILD: u64 = 30;
struct Probe {
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
struct Fixture {
    rt: Runtime,
    component: ComponentId,
    root: RegionId,
    visible: State<bool>,
    heading: State<bool>,
    count: State<i32>,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}
impl Fixture {
    fn new() -> Self {
        let mut rt = Runtime::default();
        let component = rt.component();
        let visible = rt.state_in(component, 1, || true);
        let heading = rt.state_in(component, 2, || false);
        let count = rt.state_in(component, COUNT, || 0);
        let calls = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        rt.state_in(component, PROBE, || Probe {
            calls: Rc::clone(&calls),
            drops: Rc::clone(&drops),
        });
        let root = rt.watch_in(component, (visible, heading), |cx, visible, heading| {
            cx.text(format!("before:{}", *heading.read()));
            if *visible.read() {
                let count = cx.state::<i32>(COUNT);
                let probe = cx.state::<Probe>(PROBE);
                cx.watch(CHILD, (count, probe), |cx, count, probe| {
                    let probe = probe.read();
                    probe.calls.set(probe.calls.get() + 1);
                    cx.text(format!("child:{}", *count.read()));
                    cx.watch(GRANDCHILD, (count.handle(),), |cx, count| {
                        cx.text(format!("grand:{}", *count.read()));
                    });
                });
            }
            cx.text("after");
        });
        rt.flush();
        Self {
            rt,
            component,
            root,
            visible,
            heading,
            count,
            calls,
            drops,
        }
    }
    fn child(&self) -> RegionId {
        self.rt.child(self.root, CHILD).unwrap()
    }
    fn grandchild(&self) -> RegionId {
        self.rt.child(self.child(), GRANDCHILD).unwrap()
    }
    fn button(&mut self) -> HandlerId {
        let child = self.child();
        self.rt
            .on_click(child, (self.count,), |count| count.update(|v| *v += 1))
    }
}
#[test]
fn child_only_update_does_not_replay_parent_and_preserves_interleaving() {
    let mut f = Fixture::new();
    let child = f.child();
    let grand = f.grandchild();
    assert_eq!(
        f.rt.snapshot(f.root),
        ["before:false", "child:0", "grand:0", "after"]
    );
    let button = f.button();
    assert!(f.rt.click(button));
    f.rt.flush();
    assert_eq!(f.rt.executions(f.root), Some(1));
    assert_eq!(f.rt.executions(child), Some(2));
    assert_eq!(f.rt.executions(grand), Some(2));
    assert_eq!(
        f.rt.snapshot(f.root),
        ["before:false", "child:1", "grand:1", "after"]
    );
}
#[test]
fn parent_replay_reuses_children_component_state_and_handlers() {
    let mut f = Fixture::new();
    let child = f.child();
    let grand = f.grandchild();
    let button = f.button();
    f.rt.access(f.count).update(|v| *v = 7);
    f.rt.flush();
    f.rt.access(f.heading).update(|v| *v = true);
    f.rt.flush();
    assert_eq!(f.child(), child);
    assert_eq!(f.grandchild(), grand);
    let reused: State<i32> =
        f.rt.state_in(f.component, COUNT, || panic!("state initialized twice"));
    assert_eq!(*f.rt.access(reused).read(), 7);
    assert!(f.rt.click(button));
    f.rt.flush();
    assert_eq!(
        f.rt.snapshot(f.root),
        ["before:true", "child:8", "grand:8", "after"]
    );
    assert_eq!(f.drops.get(), 0);
}
#[test]
fn parent_and_descendant_invalidations_coalesce_independent_of_enqueue_order() {
    for child_first in [false, true] {
        let mut f = Fixture::new();
        let child = f.child();
        let grand = f.grandchild();
        if child_first {
            f.rt.access(f.count).update(|v| *v += 1);
        }
        f.rt.access(f.heading).update(|v| *v = true);
        if !child_first {
            f.rt.access(f.count).update(|v| *v += 1);
        }
        f.rt.flush();
        assert_eq!(f.rt.executions(f.root), Some(2));
        assert_eq!(f.rt.executions(child), Some(2));
        assert_eq!(f.rt.executions(grand), Some(2));
        assert_eq!(f.calls.get(), 2);
    }
}
#[test]
fn parent_removal_cancels_queued_descendants_and_old_events() {
    let mut f = Fixture::new();
    let child = f.child();
    let grand = f.grandchild();
    let button = f.button();
    f.rt.access(f.count).update(|v| *v = 9); // Child/grandchild queued first.
    f.rt.access(f.visible).update(|v| *v = false);
    f.rt.flush();
    assert_eq!(
        f.calls.get(),
        1,
        "dead child executed before parent removed it"
    );
    assert_eq!(f.rt.executions(child), None);
    assert_eq!(f.rt.executions(grand), None);
    assert!(!f.rt.click(button));
    assert!(f.rt.state_alive(f.count));
    assert_eq!(
        f.drops.get(),
        0,
        "watch removal must not dispose component state"
    );
    assert_eq!(f.rt.snapshot(f.root), ["before:false", "after"]);
    f.rt.access(f.count).update(|v| *v += 1);
    assert!(f.rt.flush().is_empty());
    f.rt.access(f.visible).update(|v| *v = true);
    f.rt.flush();
    assert_ne!(f.child(), child);
    assert_ne!(f.grandchild(), grand);
    assert!(!f.rt.click(button));
    assert_eq!(
        f.rt.snapshot(f.root),
        ["before:false", "child:10", "grand:10", "after"]
    );
}
#[test]
fn component_unmount_frees_state_subscriptions_handlers_and_descendants() {
    let mut f = Fixture::new();
    let child = f.child();
    let button = f.button();
    f.rt.access(f.count).update(|v| *v += 1);
    f.rt.unmount_component(f.component);
    assert!(f.rt.flush().is_empty());
    assert!(!f.rt.state_alive(f.count));
    assert!(!f.rt.state_alive(f.visible));
    assert_eq!(f.rt.executions(child), None);
    assert!(!f.rt.click(button));
    assert_eq!(f.drops.get(), 1);
    assert_eq!(f.rt.live_counts(), (0, 0, 0, 0, 0));
    let owner = f.rt.component();
    let replacement = f.rt.state_in(owner, COUNT, || 100);
    assert_eq!(*f.rt.access(replacement).read(), 100);
    assert!(!f.rt.state_alive(f.count));
    assert!(!f.rt.click(button));
}
#[test]
fn changed_dependency_rebinds_existing_child_and_unsubscribes_old_state() {
    let mut rt = Runtime::default();
    let owner = rt.component();
    let choose_b = rt.state_in(owner, 1, || false);
    let a = rt.state_in(owner, 2, || 10);
    let b = rt.state_in(owner, 3, || 20);
    let root = rt.watch_in(owner, (choose_b,), |cx, choose_b| {
        let state = cx.state::<i32>(if *choose_b.read() { 3 } else { 2 });
        cx.watch(1, (state,), |cx, value| cx.text(value.read().to_string()));
    });
    rt.flush();
    let child = rt.child(root, 1).unwrap();
    rt.access(choose_b).update(|v| *v = true);
    rt.flush();
    assert_eq!(rt.child(root, 1), Some(child));
    assert_eq!(rt.snapshot(root), ["20"]);
    let calls = rt.executions(child);
    rt.access(a).update(|v| *v = 11);
    assert!(rt.flush().is_empty());
    assert_eq!(rt.executions(child), calls);
    rt.access(b).update(|v| *v = 21);
    rt.flush();
    assert_eq!(rt.snapshot(root), ["21"]);
}
#[test]
fn keyed_child_reorder_keeps_instances_and_changes_output_order() {
    let mut rt = Runtime::default();
    let owner = rt.component();
    let order = rt.state_in(owner, 1, || vec![10_u64, 20]);
    let a = rt.state_in(owner, 10, || 100_i32);
    rt.state_in(owner, 20, || 200_i32);
    let root = rt.watch_in(owner, (order,), |cx, order| {
        for &key in order.read().iter() {
            cx.watch(key, (cx.state::<i32>(key),), |cx, value| {
                cx.text(value.read().to_string())
            });
        }
    });
    rt.flush();
    let first = rt.child(root, 10);
    let second = rt.child(root, 20);
    rt.access(order).update(|v| v.reverse());
    rt.flush();
    assert_eq!(rt.child(root, 10), first);
    assert_eq!(rt.child(root, 20), second);
    assert_eq!(rt.snapshot(root), ["200", "100"]);
    rt.access(a).update(|v| *v = 101);
    rt.flush();
    assert_eq!(rt.snapshot(root), ["200", "101"]);
}
#[test]
fn same_child_key_in_different_parents_is_scoped() {
    let mut rt = Runtime::default();
    let owner = rt.component();
    let value = rt.state_in(owner, 1, || 0);
    let a = rt.watch_in(owner, (value,), |cx, v| {
        cx.watch(1, (v.handle(),), |cx, _| cx.text("a"))
    });
    let b = rt.watch_in(owner, (value,), |cx, v| {
        cx.watch(1, (v.handle(),), |cx, _| cx.text("b"))
    });
    rt.flush();
    assert_ne!(rt.child(a, 1), rt.child(b, 1));
    assert_eq!(rt.snapshot(a), ["a"]);
    assert_eq!(rt.snapshot(b), ["b"]);
}
#[test]
fn mixed_updates_match_full_rebuild_reference() {
    let mut f = Fixture::new();
    let mut seed = 0x12345678_u32;
    let (mut visible, mut heading, mut count) = (true, false, 0);
    for _ in 0..500 {
        for _ in 0..3 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            match seed % 3 {
                0 => {
                    visible = !visible;
                    f.rt.access(f.visible).update(|v| *v = visible);
                }
                1 => {
                    heading = !heading;
                    f.rt.access(f.heading).update(|v| *v = heading);
                }
                _ => {
                    count += 1;
                    f.rt.access(f.count).update(|v| *v = count);
                }
            }
        }
        f.rt.flush();
        let mut expected = vec![format!("before:{heading}")];
        if visible {
            expected.extend([format!("child:{count}"), format!("grand:{count}")]);
        }
        expected.push("after".into());
        assert_eq!(f.rt.snapshot(f.root), expected);
    }
}
#[test]
fn repeated_mount_and_unmount_returns_live_counts_to_zero() {
    let mut rt = Runtime::default();
    for _ in 0..100 {
        let owner = rt.component();
        let value = rt.state_in(owner, 1, || 0);
        let root = rt.watch_in(owner, (value,), |cx, value| {
            cx.watch(1, (value.handle(),), |cx, v| cx.text(v.read().to_string()))
        });
        rt.flush();
        let child = rt.child(root, 1).unwrap();
        let handler = rt.on_click(child, (value,), |v| v.update(|v| *v += 1));
        assert!(rt.click(handler));
        rt.unmount_component(owner);
        rt.flush();
        assert_eq!(rt.live_counts(), (0, 0, 0, 0, 0));
        assert!(!rt.click(handler));
    }
}

#[test]
fn lifetime_only_adopted_children_survive_parent_replay_and_are_disposed_with_it() {
    let mut rt = Runtime::default();
    let state = rt.state(|| 0);
    let parent = rt.watch((state,), |cx, v| cx.text(v.read().to_string()));
    let child = rt.watch((state,), |cx, v| cx.text(v.read().to_string()));
    rt.adopt(parent, child);
    rt.flush();
    rt.access(state).update(|v| *v += 1);
    rt.flush();
    assert_eq!(rt.executions(child), Some(2));
    rt.unmount(parent);
    assert_eq!(rt.executions(child), None);
}

#[test]
#[should_panic(expected = "duplicate child region key")]
fn duplicate_keys_under_same_parent_are_rejected() {
    let mut rt = Runtime::default();
    let value = rt.state(|| 0);
    rt.watch((value,), |cx, value| {
        cx.watch(1, (value.handle(),), |_, _| {});
        cx.watch(1, (value.handle(),), |_, _| {});
    });
    rt.flush();
}

#[test]
fn manual_child_removal_cleans_snapshot_anchors() {
    let mut f = Fixture::new();
    let child = f.child();
    f.rt.unmount(child);
    assert_eq!(f.rt.snapshot(f.root), ["before:false", "after"]);
    f.rt.access(f.heading).update(|v| *v = true);
    f.rt.flush();
    assert_ne!(f.child(), child);
    assert_eq!(
        f.rt.snapshot(f.root),
        ["before:true", "child:0", "grand:0", "after"]
    );
}
