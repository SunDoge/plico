//! Renderer-free mixed synchronous/asynchronous components on ONE Engine.
use plico_async::{AsyncUi, Input, Result, Runtime};
use plico_core::{Engine, Entity, prelude::*};

async fn counter(ui: AsyncUi, title: Input<String>) -> Result<()> {
    title.subscribe(&ui);
    let mut count: i32 = ui.state("count", || 0);
    loop {
        ui.frame(|cx| {
            title.with(|title| {
                text(cx, title.as_str()).font_size(20);
            });
            text!(cx, "Count: {count}");
            if button(cx).text("Increment").clicked() {
                count += 1;
                cx.request_rebuild();
            }
        })
        .await?;
        ui.checkpoint("count", &count)?;
    }
}
fn find(engine: &Engine, parent: Entity) -> Option<Entity> {
    let node = engine.node(parent)?;
    if matches!(node.kind, plico_core::Kind::ButtonBase) {
        return Some(parent);
    }
    node.children.iter().find_map(|&id| find(engine, id))
}
fn main() -> Result<()> {
    let mut runtime = Runtime::new();
    let title = Input::new("Local state, shared Engine".to_owned());
    let mut counter_id = None;
    runtime.render(|cx, mounts| {
        let _ = column(cx).padding(20).gap(12)
            + |cx| {
                text(cx, "Synchronous parent").font_size(24);
                counter_id = Some(mounts.mount(column(cx).key("counter"), {
                    let title = title.clone();
                    move |ui| counter(ui, title)
                }));
                text(cx, "Synchronous footer");
            };
    })?;
    runtime.pump()?;
    let button = find(&runtime.engine(), counter_id.unwrap().entity()).unwrap();
    runtime.queue_click(button);
    runtime.pump()?;
    title.set("Parent input updated".into());
    runtime.pump()?;
    assert_eq!(runtime.pump()?, 0);
    println!("mixed: sync parent/footer + async counter, same Engine, idle stops");
    println!("checkpoint snapshot: {} bytes", runtime.dump_state()?.len());
    Ok(())
}
