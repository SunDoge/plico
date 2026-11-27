use plico::widgets::{button, column, text};
use plico::{Engine, UiContext, component};
#[component]
fn settings(ui: &mut UiContext<'_>, large: bool) {
    column(ui).gap(12).padding(16).children(|ui| {
        text(ui, "Settings").font_size(if large { 28 } else { 20 });
        text(ui, "Immediate declarations, retained storage.");
        button(ui).text("Save").primary();
    })
}
fn main() -> Result<(), plico::FrameError> {
    let mut engine = Engine::default();
    println!("cold: {:?}", engine.frame(|ui| settings(ui, false))?);
    engine.frame(|ui| settings(ui, false))?; // Warm both reusable child buffers.
    println!("unchanged: {:?}", engine.frame(|ui| settings(ui, false))?);
    println!("font changed: {:?}", engine.frame(|ui| settings(ui, true))?);
    engine.set_theme(plico::Theme {
        accent: 0x8844cc,
        ..Default::default()
    });
    println!(
        "theme changed: {:?}",
        engine.frame(|ui| settings(ui, true))?
    );
    println!("tree: {:?}", engine.snapshot());
    Ok(())
}
