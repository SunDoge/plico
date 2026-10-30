//! Fixed width through OS size constraints; optional custom-border edge selection.
use plico::prelude::*;
use plico_desktop::{HostAction, HostView, WindowCommand, WindowOptions, WindowResizeEdges};

#[derive(Default)]
struct Demo {
    close: bool,
}
impl HostView for Demo {
    fn take_host_action(&mut self) -> Option<HostAction> {
        std::mem::take(&mut self.close).then_some(HostAction::Window(WindowCommand::Close))
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(24).gap(16) + |cx| {
            text(cx, "Vertical window resizing").font_size(22);
            text(cx, "Width stays at 480 logical pixels. Height can vary from 240 to 900.");
            text(cx, "Drag the top or bottom edge. Corners also resize vertically.");
            if button(cx).text("Close").clicked() {
                self.close = true;
            }
        };
    }
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    let custom = std::env::var_os("PLICO_CUSTOM_DECORATIONS").is_some();
    plico_desktop::run_host_view_with_options(
        Demo::default(),
        WindowOptions::new()
            .title("Plico window resize constraints")
            .initial_inner_size(480, 420)
            .min_inner_size(480, 240)
            .max_inner_size(480, 900)
            .resize_edges(WindowResizeEdges::TOP | WindowResizeEdges::BOTTOM)
            .decorations(!custom),
    )
}
