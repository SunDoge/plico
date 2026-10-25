//! Host-managed storage bytes can be written to disk/localStorage outside show.
use plico_core::{Engine, UiContext, prelude::*};
fn page(cx: &mut UiContext) {
    let count = cx.persisted_state("counter", || 0_u32);
    text!(cx, "Visits: {}", count.get());
    count.update(|n| *n += 1);
}
fn main() {
    let mut engine = Engine::default();
    engine.frame(page).unwrap();
    engine.frame(|_| {}).unwrap(); // Route unmounts; storage remains.
    engine.frame(page).unwrap();
    let bytes = engine.dump_persisted().unwrap();
    // A host can store these bytes between process launches.
    let mut restored = Engine::default();
    restored.load_persisted(&bytes).unwrap();
    restored
        .frame(|cx| assert_eq!(cx.persisted_state("counter", || 0_u32).get(), 2))
        .unwrap();
    println!("Persisted counter survives page unmount and Engine recreation.");
}
