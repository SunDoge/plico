//! Deterministic coverage probe: a rect offset by half a pixel horizontally
//! must produce exactly 50% coverage on the covered column pair. The output
//! byte tells us whether compositing is linear or gamma-encoded.
use plico_renderer::{PaintScene, Scene};
use vello_common::kurbo::{Affine, Rect};
use vello_common::peniko::{Color, Fill};

fn main() {
    let width = 8u32;
    let height = 4u32;
    let bg = Color::from_rgba8(244, 244, 245, 255);
    let ink = Color::from_rgba8(0x20, 0x20, 0x24, 255);
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        bg,
        None,
        &Rect::new(0., 0., 8., 4.),
    );
    // Exact half coverage: rect from x=0.5 to x=1.5 covers column 0 fully and
    // column 1 fully? No — we want one column at exactly 50%: x=1.0..1.5.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        ink,
        None,
        &Rect::new(1.0, 0.0, 1.5, 4.0),
    );
    plico_shell::renderer::render_to_png(&scene, width, height, bg, "/tmp/gamma-probe.png")
        .expect("offscreen render");
    println!("wrote /tmp/gamma-probe.png (column 1 should be exactly 50% coverage)");
}
