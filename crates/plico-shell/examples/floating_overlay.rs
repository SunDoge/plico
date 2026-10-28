//! Transparent floating window showing sample OCR bounding boxes.
//!
//! Stop it with Ctrl-C in the launching terminal. On Wayland, this creates an
//! ordinary window: stacking, position, and input passthrough are compositor
//! dependent; it does not create a layer-shell surface.
use kurbo::{Affine, Rect, Stroke};
use peniko::Color;
use plico_shell::{
    FrameSource, PaintScene, Scene, TextContext, WindowOptions, run_window_with_options,
};

struct Overlay;

impl FrameSource for Overlay {
    fn build_frame(
        &mut self,
        _: &mut TextContext,
        _: u32,
        _: u32,
    ) -> Vec<plico_shell::layout::PlacedNode> {
        Vec::new()
    }

    fn base_color(&self) -> Color {
        Color::TRANSPARENT
    }

    fn paint_scene(
        &mut self,
        scene: &mut Scene,
        _: &[plico_shell::layout::PlacedNode],
        _: &mut TextContext,
        _: (u32, u32),
        scale: f64,
    ) {
        scene.reset();
        for rect in [
            Rect::new(35., 35., 350., 76.),
            Rect::new(35., 90., 270., 131.),
        ] {
            scene.stroke(
                &Stroke::new(3.),
                Affine::scale(scale),
                Color::from_rgb8(72, 190, 255),
                None,
                &rect,
            );
        }
    }
}

fn main() -> plico_shell::Result<()> {
    println!("Floating overlay: stop with Ctrl-C in this terminal.");
    run_window_with_options(
        Box::new(Overlay),
        WindowOptions::floating_overlay()
            .title("Plico floating overlay")
            .initial_inner_size(420, 180)
            .resizable(false),
    )
}
