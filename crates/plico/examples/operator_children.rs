//! Native Rust + closure experiment. Existing applications are not migrated.
use plico::{Engine, prelude::*};

#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext, expanded: bool, count: &mut usize) {
    let title = "Settings";
    column(cx).gap(16).padding(20) + |cx| {
        text(cx, title).font_size(24).weight(650);
        if expanded {
            row(cx).gap(8) + |cx| {
                button(cx).text("Save").primary();
                text(cx, "Nested");
            };
        }
        for id in 0..3u64 {
            row(cx).key(id).gap(8) + |cx| {
                text!(cx, "Item {id}");
            };
        }
        if button(cx).text("Increment").clicked() {
            *count += 1;
        }
    };
}

fn main() -> Result<(), plico::FrameError> {
    let mut engine = Engine::default();
    let mut count = 0;
    engine.frame_declarations(|cx| settings(cx, true, &mut count))?;
    let cold = serde_json::to_value(engine.snapshot()).unwrap();
    let warm = engine.frame_declarations(|cx| settings(cx, true, &mut count))?;
    assert_eq!((warm.created, warm.removed, warm.text_writes), (0, 0, 0));
    assert_eq!(serde_json::to_value(engine.snapshot()).unwrap(), cold);
    engine.frame_declarations(|cx| settings(cx, false, &mut count))?;
    assert!(engine.last_stats().removed > 0);
    println!("operator children: nested, conditional and keyed declarations passed");
    Ok(())
}
