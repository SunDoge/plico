//! Experimental native children syntax; see docs/experiments/component-format.md.
use plico_core::{Engine, prelude::*};

#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext, expanded: bool) {
    // Layout headers stay on one line when they fit.
    column(cx).gap(16).padding(20).children(|cx| {
        text(cx, "Settings").font_size(24).weight(650);
        if expanded {
            row(cx).gap(8).children(|cx| {
                button(cx).text("Optional");
            });
        }
        row(cx).gap(8).children(|cx| {
            button(cx).text("Save").primary(); // Keep this comment.
            button(cx).text("Cancel").ghost();
        });
    });
}

fn main() -> Result<(), plico_core::FrameError> {
    let mut engine = Engine::default();
    engine.frame_declarations(|cx| settings(cx, true))?;
    engine.frame_declarations(|cx| settings(cx, false))?;
    println!("native children compiled and declared successfully");
    Ok(())
}
