use plico_core::{Engine, UiContext, component};
#[component]
fn settings(ui: &mut UiContext<'_>, large: bool) {
    ui.column().gap(12).padding(16).children(|ui| {
        ui.text("Settings").font_size(if large { 28 } else { 20 });
        ui.text("Immediate declarations, retained storage.");
        ui.button("Save").primary();
    })
}
fn main() -> Result<(), plico_core::FrameError> {
    let mut engine = Engine::default();
    println!("cold: {:?}", engine.frame(|ui| settings(ui, false))?);
    engine.frame(|ui| settings(ui, false))?; // Warm both reusable child buffers.
    println!("unchanged: {:?}", engine.frame(|ui| settings(ui, false))?);
    println!("font changed: {:?}", engine.frame(|ui| settings(ui, true))?);
    engine.set_theme(plico_core::Theme {
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
