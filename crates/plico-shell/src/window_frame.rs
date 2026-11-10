//! Native client-area clipping, independent of platform window decorations.
use kurbo::{Affine, Rect};
use peniko::Color;
use plico_renderer::{PaintCommand, Scene};

/// Linux decorated windows leave the outer contour to the window system
/// (server decorations or winit's negotiated client decorations). Do not cut
/// transparent holes into their rectangular content surface as well.
/// Explicit frameless windows retain the renderer fallback.
pub(crate) fn client_corner_radius(requested: u32, decorations: bool) -> u32 {
    if cfg!(target_os = "linux") && decorations {
        0
    } else {
        requested
    }
}

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
            clip: plico_renderer::PaintShape::record(&clip, 0.1),
        },
    );
    if base != Color::TRANSPARENT {
        scene.commands.insert(
            1,
            PaintCommand::Fill {
                fill: peniko::Fill::NonZero,
                transform: Affine::IDENTITY,
                brush: base.into(),
                brush_transform: None,
                shape: plico_renderer::PaintShape::record(
                    &Rect::new(0., 0., size[0] as f64, size[1] as f64),
                    0.1,
                ),
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
    #[cfg(all(target_os = "linux", feature = "wgpu"))]
    #[test]
    #[ignore = "requires an X11 display and working wgpu adapter; Xvfb is sufficient"]
    fn native_linux_decorations_do_not_clip_the_client_area_twice() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use winit::platform::x11::EventLoopBuilderExtX11;
        use winit::{
            application::ApplicationHandler,
            event::WindowEvent,
            event_loop::{ActiveEventLoop, EventLoop},
            window::WindowId,
        };
        struct Probe(Arc<AtomicBool>);
        impl ApplicationHandler for Probe {
            fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
                // Xvfb may expose only opaque surfaces. This test deliberately
                // exercises the decorated path: a requested radius must not
                // force an alpha swapchain or client-area clipping.
                let options = crate::WindowOptions::new()
                    .decorations(true)
                    .corner_radius(16)
                    .initial_inner_size(160, 120)
                    .renderer(crate::RenderBackend::Wgpu);
                let mut shell = crate::shell::Shell::create(event_loop, &options).unwrap();
                assert!(shell.window().is_decorated());
                assert_eq!(shell.prepare_window_frame(Color::BLACK), Color::BLACK);
                assert!(shell.scene.commands.is_empty());
                self.0.store(true, Ordering::SeqCst);
                event_loop.exit();
            }
            fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        }
        let checked = Arc::new(AtomicBool::new(false));
        let mut builder = EventLoop::builder();
        builder.with_x11().with_any_thread(true);
        builder
            .build()
            .unwrap()
            .run_app(Probe(checked.clone()))
            .unwrap();
        assert!(checked.load(Ordering::SeqCst));
    }
    #[test]
    fn decorated_linux_windows_delegate_the_contour_but_frameless_keeps_fallback() {
        assert_eq!(client_corner_radius(16, false), 16);
        assert_eq!(client_corner_radius(0, true), 0);
        assert_eq!(
            client_corner_radius(16, true),
            if cfg!(target_os = "linux") { 0 } else { 16 }
        );
    }
    #[test]
    fn rounded_window_has_transparent_corners_at_both_scales() {
        let folder = tempfile::tempdir().unwrap();
        for scale in [1, 2] {
            let mut scene = Scene::new();
            let size = [100 * scale, 80 * scale];
            scene.fill(
                peniko::Fill::NonZero,
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
