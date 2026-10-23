//! Native Rust operator children with stable component identity.
use plico_core::prelude::*;

#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext, extra: bool) {
    column(cx).padding(20) + |cx| {
        text(cx, "Settings").font_size(24);
        if extra {
            row(cx).gap(8) + |cx| {
                button(cx).text("Optional");
            };
        }
        row(cx).gap(8) + |cx| {
            button(cx).text("Save").primary();
            text(cx, "Nested").font_size(14);
        };
        match extra {
            true => {
                text(cx, "Expanded");
            }
            false => {
                text(cx, "Compact");
            }
        }
    };
}
fn main() -> Result<(), plico_core::FrameError> {
    let mut engine = plico_core::Engine::default();
    engine.frame_declarations(|cx| settings(cx, true))?;
    engine.frame_declarations(|cx| settings(cx, false))?;
    Ok(())
}
