//! Ordinary synchronous components with private state, no App fields.
use plico_core::{Engine, UiContext, component, prelude::*};

#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext) {
    let count = cx.state(|| 0_u32);
    column(cx).gap(12.).padding(24.) + |cx| {
        text!(cx, "Count: {}", count.get());
        if button(cx).text("Increment").clicked() {
            count.update(|n| *n += 1);
        }
    };
}

fn main() {
    let mut engine = Engine::default();
    engine.frame(counter).unwrap();
    engine.frame(counter).unwrap();
    println!("Synchronous counter declared twice; private state lives in Engine.");
}
