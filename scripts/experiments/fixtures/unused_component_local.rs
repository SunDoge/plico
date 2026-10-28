#![deny(unused_variables)]
use plico::prelude::*;
#[component]
pub fn unused_local(cx: &mut UiContext) {
    let unused_local = 42;
    text(cx, "A real use of the context");
}
