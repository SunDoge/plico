//! Application widgets are ordinary context-first Rust functions.
#[allow(dead_code)]
#[path = "../tests/support/icons.rs"]
mod icons;
use plico_core::{Engine, prelude::*};

#[component]
fn status_action(cx: &mut UiContext, label: &str) -> Response {
    let m = cx.theme().metrics;
    button_base(cx, "Status action")
        .padding(m.space(3))
        .gap(m.space(2))
        .radius(m.radius as f32)
        .background(Color::Selection)
        .children(|cx| {
            icon(cx, &icons::FOLDER).color(Color::Muted);
            text(cx, label).color(Color::Accent);
        })
        .response
}

#[component]
fn example(cx: &mut UiContext, enabled: &mut bool) {
    if button(cx).text("Save").primary().clicked() {
        text(cx, "Saved");
    }
    status_action(cx, "Custom component");
    switch(cx, "Notifications", enabled)
        .disabled(false)
        .motion(Motion::Smooth);
    let checked = checkbox(cx, "Enabled", enabled).gap(8).response();
    assert!(checked.enabled());
    let count = 42;
    text!(cx, "Count: {count}").font_size(18);
    let response = button(cx).text("Inspect").ghost().children(|cx| {
        icon(cx, &icons::SEARCH);
        text(cx, "Inspect project");
        text(cx, "Details").color(Color::Muted);
    });
    assert!(response.response.enabled());
}
fn main() {
    let mut engine = Engine::default();
    let mut enabled = false;
    engine
        .frame_declarations(|cx| example(cx, &mut enabled))
        .unwrap();
}
