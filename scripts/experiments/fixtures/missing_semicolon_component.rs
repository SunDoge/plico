use plico_core::prelude::*;
use plico_core::component;
#[component]
fn invalid(cx: &mut UiContext) {
    column(cx);; {
        text(cx, "First")
        text(cx, "Second");
    }
}
