//! Native client-area clipping, independent of platform window decorations.
use plico_renderer::{PaintCommand, Scene};
use vello_common::{
    kurbo::{Affine, Rect, Shape},
    peniko::Color,
};

/// Clip a completed physical-pixel scene and select a transparent renderer clear.
///
/// The scene must be freshly assembled, before calling this once per frame.
/// No native input-region shaping is performed.
pub fn clip_window_frame(scene: &mut Scene, size: [u32; 2], radius: f64, base: Color) -> Color {
    let radius = radius.min(f64::from(size[0].min(size[1])) * 0.5);
    if !radius.is_finite() || radius <= 0. {
        return base;
    }
    let clip = Rect::new(0., 0., size[0] as f64, size[1] as f64).to_rounded_rect(radius);
    scene.commands.insert(
        0,
        PaintCommand::PushClip {
            transform: Affine::IDENTITY,
            clip: clip.to_path(0.1),
        },
    );
    if base != Color::TRANSPARENT {
        scene.commands.insert(
            1,
            PaintCommand::Fill {
                fill: vello_common::peniko::Fill::NonZero,
                transform: Affine::IDENTITY,
                brush: base.into(),
                brush_transform: None,
                shape: Rect::new(0., 0., size[0] as f64, size[1] as f64).to_path(0.1),
            },
        );
    }
    scene.commands.push(PaintCommand::PopLayer);
    Color::TRANSPARENT
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::PaintScene;
    #[test]
    fn rounded_window_has_transparent_corners_at_both_scales() {
        let folder = tempfile::tempdir().unwrap();
        for scale in [1, 2] {
            let mut scene = Scene::new();
            let size = [100 * scale, 80 * scale];
            scene.fill(
                vello_common::peniko::Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(30, 40, 50),
                None,
                &Rect::new(0., 0., size[0] as f64, size[1] as f64),
            );
            let base = clip_window_frame(&mut scene, size, 12. * scale as f64, Color::BLACK);
            assert_eq!(base, Color::TRANSPARENT);
            let path = folder.path().join(format!("rounded-{scale}.png"));
            crate::renderer::render_to_png(&scene, size[0], size[1], base, path.to_str().unwrap())
                .unwrap();
            let image = image::open(path).unwrap().into_rgba8();
            assert_eq!(image.get_pixel(0, 0)[3], 0);
            assert_eq!(image.get_pixel(size[0] / 2, size[1] / 2)[3], 255);
            assert_eq!(image.get_pixel(size[0] / 2, 0)[3], 255);
        }
    }
}
