use plico::prelude::*;
use plico_render::{Composition, Options};

fn main() -> Result<(), plico_render::Error> {
    let mut composition = Composition::new(1280, 720, 90, |ui, frame| {
        let _ = column(ui)
            .width_percent(100.)
            .height_percent(100.)
            .align(Align::Center)
            .justify(Justify::Center)
            + |ui| {
                text(ui, "Hello from Plico")
                    .font_size(64)
                    .opacity(frame.interpolate(&[0., 20.], &[0., 1.]) as f32);
            };
    })
    .fps(30.)
    .scale(2.);

    plico_render::video(&mut composition, "hello.mp4", Options::default())
}
