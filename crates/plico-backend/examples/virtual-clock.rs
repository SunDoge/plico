//! External logical time drives UI frames without sleeping or opening a window.
use plico_backend::{Source, View};
use plico_core::{Clock, Ui, VirtualClock, component};
use plico_renderer::{FrameSource, TextContext};
use std::time::Duration;
struct Timer;
impl View for Timer {
    #[component]
    fn show(&mut self, ui: &mut Ui<'_>) {
        ui.column().gap(8).children(|ui| {
            ui.text(format!("{} ms", ui.now().as_millis()));
            if ui.now() < Duration::from_millis(200) {
                ui.request_animation_frame();
            }
        })
    }
}
fn main() -> Result<(), plico_core::ClockError> {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Timer, clock.clone());
    let mut text = TextContext::new();
    for step in [0, 100, 0, 100] {
        clock.advance(Duration::from_millis(step))?;
        let nodes = source.build_frame(&mut text, 640, 300);
        let labels: Vec<_> = nodes
            .iter()
            .filter_map(|n| n.paint.text.as_deref())
            .collect();
        println!(
            "{:?}: {labels:?}, next_frame={}",
            clock.now(),
            source.has_anim()
        );
    }
    assert!(!source.has_anim());
    Ok(())
}
