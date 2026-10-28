//! Nested-region lifecycle demo. Logical text output only; no renderer/window.
use plico_reactivity_compare::Runtime;

fn main() {
    let mut rt = Runtime::default();
    let component = rt.component();
    let visible = rt.state_in(component, 1, || true);
    let title = rt.state_in(component, 2, || 0);
    let count = rt.state_in(component, 3, || 0);
    let root = rt.watch_in(component, (visible, title), |cx, visible, title| {
        cx.text(format!("Header {}", *title.read()));
        if *visible.read() {
            // Resolve a component-owned slot, then explicitly subscribe the child.
            let count = cx.state::<i32>(3);
            cx.watch(10, (count,), |cx, count| {
                cx.text(format!("Count: {}", *count.read()));
                cx.watch(20, (count.handle(),), |cx, count| {
                    cx.text(format!("Double: {}", *count.read() * 2));
                });
            });
        }
        cx.text("Footer");
    });
    rt.flush();
    let child = rt.child(root, 10).unwrap();
    let grandchild = rt.child(child, 20).unwrap();
    let click = rt.on_click(child, (count,), |count| count.update(|v| *v += 1));
    println!("initial: {:?}", rt.snapshot(root));

    assert!(rt.click(click));
    rt.flush();
    assert_eq!(rt.executions(root), Some(1));
    println!("child update, parent executions=1: {:?}", rt.snapshot(root));

    rt.access(title).update(|v| *v += 1);
    rt.flush();
    assert_eq!(rt.child(root, 10), Some(child));
    assert_eq!(rt.child(child, 20), Some(grandchild));
    println!("parent replay, child IDs retained: {:?}", rt.snapshot(root));

    assert!(rt.click(click)); // Child queued before parent removes it.
    rt.access(visible).update(|v| *v = false);
    rt.flush();
    assert_eq!(rt.executions(child), None);
    assert!(!rt.click(click));
    assert!(rt.state_alive(count));
    println!(
        "hidden, old event rejected, state alive: {:?}",
        rt.snapshot(root)
    );

    rt.access(visible).update(|v| *v = true);
    rt.flush();
    assert_ne!(rt.child(root, 10), Some(child));
    println!("remounted, count retained: {:?}", rt.snapshot(root));

    rt.unmount_component(component);
    rt.flush();
    assert!(!rt.state_alive(count));
    assert_eq!(rt.live_counts(), (0, 0, 0, 0, 0));
    println!("component unmounted: live counts {:?}", rt.live_counts());
}
