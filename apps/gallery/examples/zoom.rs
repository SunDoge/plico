//! Native host zoom is separate from the renderer-free application state.
use plico::prelude::*;
use plico_desktop::{Source, View, WindowOptions, ZoomController, ZoomSource};
struct Demo {
    zoom: ZoomController,
    count: u32,
    editor: plico::TextBuffer,
}
impl View for Demo {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        let factor = self.zoom.factor();
        column(cx).width_percent(100.).height_percent(100.).padding(28).gap(20).scroll_y(true) + |cx| {
            text(cx, "Window zoom").font_size(28).weight(700);
            text(cx, "Native scale, layout, hit testing and IME geometry share one zoom factor.").font_size(15).color(Color::Muted);
            row(cx).gap(10).align(Align::Center) + |cx| {
                if button(cx).text("−").aria_label("Zoom out").clicked() {
                    self.zoom.set((factor - 0.25).max(0.5));
                }
                text(cx, format!("{:.0}%", factor * 100.)).font_size(20).weight(600);
                if button(cx).text("+").aria_label("Zoom in").clicked() {
                    self.zoom.set((factor + 0.25).min(3.));
                }
                if button(cx).text("Reset").clicked() {
                    self.zoom.set(1.);
                }
            };
            text(cx, "Ctrl + / Ctrl − / Ctrl 0 · macOS also supports Command").font_size(14).color(Color::Muted);
            column(cx).padding(22).gap(16).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
                text(cx, "Try the controls at every scale").font_size(20).weight(600);
                if button(cx).text(format!("Clicked {} times", self.count)).primary().clicked() {
                    self.count += 1;
                }
                text_input(cx, "Zoomed editor", &self.editor).placeholder("Type, select and try an IME here");
                for i in 0u64..12 {
                    cx.keyed(i, |cx| {
                        text(cx, format!("Scrollable row {i:02}")).font_size(15);
                    });
                }
            };
        };
    }
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    let zoom = ZoomController::default();
    let source = Source::new(Demo {
        zoom: zoom.clone(),
        count: 0,
        editor: Default::default(),
    });
    plico_desktop::run_source_with_options(
        ZoomSource::with_controller(Box::new(source), zoom),
        WindowOptions::new()
            .title("Plico gallery · Window zoom")
            .initial_inner_size(900, 760),
    )
}
