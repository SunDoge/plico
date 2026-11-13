use plico::prelude::*;
use plico_desktop::{HostView, SurfaceSource, WindowOptions};
use plico_water::{WaterController, WaterSurface};

mod water_icons;

struct WaterDemo {
    water: WaterController,
    message: &'static str,
    toolbar_offset: (f64, f64),
}

const TOOLBAR_TOP: f64 = 210.;
const TOOLBAR_WIDTH: f64 = 360.;
const TOOLBAR_HEIGHT: f64 = 46.;
const TOOLBAR_MARGIN: f64 = 12.;

impl HostView for WaterDemo {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(18).gap(12).background(Color::Literal(0x09131c))
        + |cx|
        {
            row(cx).width_percent(100.).align(Align::Center).gap(12) + |cx| {
                text(cx, "GPU Water").font_size(20).color(Color::Literal(0xe7f7ff));
                text(cx, "Drag across the surface to make waves").font_size(13).color(Color::Literal(0x8ca9b8));
            };
            column(cx).key("water").width_percent(100.).height(0.).grow(1.).min_height(320.).radius(18.).clip(true)
                .border(1., Color::Literal(0x315267)) + |cx|
            {
                let [water_width, water_height] = cx.layout_size().unwrap_or([604., 392.]);
                let horizontal_limit = ((f64::from(water_width) - TOOLBAR_WIDTH) * 0.5
                    - TOOLBAR_MARGIN).max(0.);
                let vertical_min = TOOLBAR_MARGIN - TOOLBAR_TOP;
                let vertical_max = (f64::from(water_height)
                    - TOOLBAR_TOP
                    - TOOLBAR_HEIGHT
                    - TOOLBAR_MARGIN).max(vertical_min);
                self.toolbar_offset.0 = self.toolbar_offset.0.clamp(-horizontal_limit, horizontal_limit);
                self.toolbar_offset.1 =
                    self.toolbar_offset.1.clamp(vertical_min, vertical_max);

                row(cx).absolute(0., TOOLBAR_TOP as f32).width_percent(100.).justify(Justify::Center) + |cx| {
                    row(cx).gap(9).padding(7).radius(14.).background(Color::Literal(0x152a3b)).translate(
                        self.toolbar_offset.0 as f32, self.toolbar_offset.1 as f32).background_alpha(0.82).border(1., Color::Literal(0x7895a6))
                    + |cx|
                    {
                        column(cx).width(24.).height(32.).radius(8.)
                            .background(Color::Literal(0x294354)) + |cx|
                        {
                            drag_area_content(
                                cx,
                                "Move water controls",
                                |cx| {
                                    row(cx).width_percent(100.).height_percent(100.)
                                        .align(Align::Center).justify(Justify::Center) + |cx|
                                    {
                                        icon(cx, &water_icons::GRIP_VERTICAL).size(15., 15.).color(Color::Literal(0x9bb2c0));
                                    };
                                },
                                &mut |dx, dy| {
                                    self.toolbar_offset.0 =
                                        (self.toolbar_offset.0 + dx).clamp(-horizontal_limit,
                                            horizontal_limit);
                                    self.toolbar_offset.1 =
                                        (self.toolbar_offset.1 + dy)
                                            .clamp(vertical_min, vertical_max);
                                },
                            );
                        };
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
                text(cx, self.message).absolute(22., 20.).padding(7).radius(9.).font_size(12)
                    .color(Color::Literal(0xe4f5fb)).background(Color::Literal(0x102332))
                    .background_alpha(0.76);
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
                toolbar_offset: (0., 0.),
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
