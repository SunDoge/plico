//! Headless nested watch driving real Engine declarations, Taffy and CPU Scene.
use plico_reactivity_compare::{Runtime, engine_bridge::EngineBridge};
use plico_renderer::TextContext;
fn main() {
    let mut runtime = Runtime::default();
    let owner = runtime.component();
    let visible = runtime.state_in(owner, 1, || true);
    let count = runtime.state_in(owner, 2, || 0);
    let root = runtime.watch_in(owner, (visible,), |cx, visible| {
        cx.text("Header");
        if *visible.read() {
            cx.watch(10, (cx.state::<i32>(2),), |cx, count| {
                cx.text(format!("Count {}", *count.read()));
                cx.watch(20, (count.handle(),), |cx, count| {
                    cx.text(format!("Double {}", *count.read() * 2))
                });
            });
        }
        cx.text("Footer");
    });
    runtime.flush();
    let mut text = TextContext::new();
    let mut partial = EngineBridge::new(&runtime, root, &mut text);
    let mut reference = EngineBridge::new(&runtime, root, &mut text);
    runtime.access(count).update(|v| *v += 1);
    partial.apply(&mut runtime, &mut text);
    reference.rebuild(&runtime, &mut text);
    partial.encode(&mut text);
    reference.encode(&mut text);
    assert!(partial.scene.commands == reference.scene.commands);
    println!(
        "child update: declared {} nodes vs full {}; Scene equal",
        partial
            .last_passes
            .iter()
            .map(|(_, stats)| stats.visited)
            .sum::<usize>(),
        reference.source.engine.last_stats().visited
    );
    runtime.access(visible).update(|v| *v = false);
    partial.apply(&mut runtime, &mut text);
    reference.rebuild(&runtime, &mut text);
    partial.encode(&mut text);
    reference.encode(&mut text);
    assert!(partial.scene.commands == reference.scene.commands);
    println!(
        "branch removed: {:?}; state alive: {}",
        partial.source.experiment_live_counts(),
        runtime.state_alive(count)
    );
    runtime.access(visible).update(|v| *v = true);
    partial.apply(&mut runtime, &mut text);
    reference.rebuild(&runtime, &mut text);
    partial.encode(&mut text);
    reference.encode(&mut text);
    assert!(partial.scene.commands == reference.scene.commands);
    println!("remount: {:?}; Scene equal", runtime.snapshot(root));
}
