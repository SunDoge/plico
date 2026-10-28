//! Static native input inside a modal. No serializable input/credential state.
use plico::prelude::*;
use plico_desktop::{View, WindowOptions};
#[derive(Default)]
struct Demo {
    dialog: ModalState,
    name: TextBuffer,
    saved: String,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .padding(32)
            .gap(20)
            .width_percent(100.)
            .height_percent(100.)
            .background(Color::Surface)
            .children(|cx| {
                text(cx, "Project settings").font_size(28).weight(650);
                text(
                    cx,
                    if self.saved.is_empty() {
                        "Give your project a name."
                    } else {
                        self.saved.as_str()
                    },
                )
                .color(Color::Muted);
                if button(cx).text("Rename project").primary().clicked() {
                    self.name.set(self.saved.clone().into());
                    self.dialog.open();
                }
                modal(cx, &mut self.dialog)
                    .title("Rename project")
                    .children(|cx| {
                        text(cx, "Project name").font_size(12).color(Color::Muted);
                        let input = text_input(cx, "Project name", &self.name)
                            .width_percent(100.)
                            .placeholder("A new project")
                            .response();
                        row(cx)
                            .gap(8)
                            .justify(Justify::End)
                            .wrap(true)
                            .children(|cx| {
                                if button(cx).text("Cancel").ghost().clicked() {
                                    cx.close_modal();
                                }
                                if button(cx)
                                    .text("Save name")
                                    .primary()
                                    .disabled(self.name.is_empty())
                                    .clicked()
                                    || input.submitted && !self.name.is_empty()
                                {
                                    self.saved = self.name.text().unwrap_or_default();
                                    cx.close_modal();
                                }
                            })
                    });
            })
    }
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run_view_with_options(
        Demo::default(),
        WindowOptions::new().title("Plico · Modal"),
    )
}
