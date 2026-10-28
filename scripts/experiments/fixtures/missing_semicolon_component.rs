use plico::prelude::*;
use plico::component;
#[component]
fn invalid(cx: &mut UiContext) {
    column(cx);; {
        text(cx, "First")
        text(cx, "Second");
    }
}
