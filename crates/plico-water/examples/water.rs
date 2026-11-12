use plico::prelude::*;
use plico_desktop::{HostView, SurfaceSource, WindowOptions};
use plico_water::{WaterController, WaterSurface};

struct WaterDemo {
    water: WaterController,
    message: &'static str,
}

impl HostView for WaterDemo {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(18).gap(12)
            .background(Color::Literal(0x09131c)) + |cx| {
            row(cx).width_percent(100.).align(Align::Center).gap(12) + |cx| {
                text(cx, "GPU Water").font_size(20).color(Color::Literal(0xe7f7ff));
                text(cx, "Drag across the surface to make waves")
                    .font_size(13).color(Color::Literal(0x8ca9b8));
            };
            column(cx).key("water").width_percent(100.).height(0.).grow(1.)
                .min_height(320.).radius(18.).clip(true)
                .border(1., Color::Literal(0x315267)) + |cx| {
                row(cx).absolute(0., 210.).width_percent(100.).justify(Justify::Center) + |cx| {
                    row(cx).gap(9).padding(7).radius(14.)
                        .background(Color::Literal(0x152a3b)).background_alpha(0.82)
                        .border(1., Color::Literal(0x7895a6)) + |cx| {
                        if button(cx).text("Soft ripple").clicked() {
                            self.water.drop_at([0.28, 0.36], 0.05, 0.012);
                            self.message = "Soft ripple at upper left";
                        }
                        if button(cx).text("Deep drop").primary().clicked() {
                            self.water.drop_at([0.68, 0.52], 0.07, 0.034);
                            self.message = "Deep drop beside the sphere";
                        }
                        if button(cx).text("Reverse").clicked() {
                            self.water.drop_at([0.5, 0.72], 0.09, -0.026);
                            self.message = "Reverse displacement near the front";
                        }
                    };
                };
                text(cx, self.message).absolute(22., 20.).padding(7).radius(9.)
                    .font_size(12).color(Color::Literal(0xe4f5fb))
                    .background(Color::Literal(0x102332)).background_alpha(0.76);
            };
        };
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (water, surface) = WaterSurface::controlled();
    plico_desktop::run_source_with_options(
        SurfaceSource::new(
            WaterDemo {
                water,
                message: "The buttons are ordinary clickable Plico widgets",
            },
            surface,
            "water",
        ),
        WindowOptions::new()
            .title("Plico GPU Water")
            .initial_inner_size(960, 680)
            .min_inner_size(640, 480),
    )?;
    Ok(())
}
