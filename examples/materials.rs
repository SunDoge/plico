//! Framework-only material demo: cargo run --example materials
use plico::prelude::*;
#[derive(Default)]
struct Demo {
    opaque: bool,
    reduce_motion: bool,
}
impl plico_dev::App for Demo {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).position(Position::Relative).width_percent(100.).height_percent(100.)
            .background(Color::Literal(0xd4e4ed)).padding(48).gap(24) + |cx|
        {
            for (x, y, color) in [
                (24., 96., 0xf3b693),
                (260., 160., 0x74b9d9),
                (520., 48., 0xa9c6af),
            ] {
                cx.keyed(color as u64, |cx| {
                    column(cx).absolute(x, y).width(300.).height(300.).radius(150.)
                        .background(Color::Literal(color)).end();
                });
            }
            column(cx).width(500.).padding(28).gap(18).radius(24.).material(LiquidGlass {
                enabled: !self.opaque,
                reduce_motion: self.reduce_motion,
                ..Default::default()
            }).elevation(Elevation::OVERLAY) + |cx|
            {
                text(cx, "Plico materials").font_size(30).weight(700);
                text(cx, "Curved glass, sharp foreground and layered shadows.");
                switch(cx, "Opaque fallback", &mut self.opaque);
                switch(cx, "Reduce motion", &mut self.reduce_motion);
                button(cx).text("A native control");
            };
        };
    }
}
fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run(Demo::default(), "Plico materials")
}
