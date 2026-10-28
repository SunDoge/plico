use plico::{component, prelude::*};
#[component]
fn retired(cx: &mut UiContext) {
    column(cx);; {
        text(cx, "must not silently become a sibling");
    }
}
