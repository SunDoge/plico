//! Native client-area clipping, independent of platform window decorations.
use kurbo::{Affine, Rect};
use peniko::Color;
use plico_renderer::{PaintCommand, Scene, style::CornerRadii};

/// Logical room reserved around Linux application content for the client-side
/// resize target and compositor-style shadow.
pub(crate) const LINUX_CSD_INSET: f64 = 14.;

/// Linux CSD and Windows DWM own their outer contours. MyGO likewise avoids a
/// second client clip there: it would disagree with the native border/shadow.
pub(crate) fn client_corner_radius(requested: u32, decorations: bool) -> u32 {
    if cfg!(any(target_os = "linux", target_os = "windows")) {
        0
    } else {
        let _ = decorations;
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

/// Wrap a completed physical-pixel application scene in Plico's pure-Rust
/// Linux client-side frame. The outer surface stays transparent; content,
/// semantics and input use coordinates relative to the inset rectangle.
pub(crate) fn decorate_linux_client_scene(
    scene: &mut Scene,
    surface_size: [u32; 2],
    inset: f64,
    radius: f64,
    scale: f64,
    base: Color,
    has_shadow: bool,
) -> Color {
    let inset = (inset * scale).max(0.);
    if inset <= 0. {
        return base;
    }
    let width = f64::from(surface_size[0]);
    let height = f64::from(surface_size[1]);
    let content = Rect::new(inset, inset, width - inset, height - inset);
    if content.width() <= 0. || content.height() <= 0. {
        return Color::TRANSPARENT;
    }
    let radius = (radius * scale)
        .max(0.)
        .min(content.width().min(content.height()) * 0.5);
    let contour = plico_renderer::corner_shape::corner_shape(
        content,
        CornerRadii::uniform(radius as f32),
        1.,
    );
    let application = std::mem::replace(scene, Scene::new());
    if has_shadow {
        scene.commands.push(PaintCommand::BoxShadow {
            transform: Affine::translate((0., scale)),
            rect: content,
            brush: Color::from_rgba8(28, 24, 38, 72),
            radius,
            std_dev: 6.5 * scale,
            corner_smoothing: 1.,
        });
    }
    scene.commands.push(PaintCommand::PushClip {
        transform: Affine::IDENTITY,
        clip: contour.clone(),
    });
    if base != Color::TRANSPARENT {
        scene.commands.push(PaintCommand::Fill {
            fill: peniko::Fill::NonZero,
            transform: Affine::IDENTITY,
            brush: base.into(),
            brush_transform: None,
            shape: contour.clone(),
        });
    }
    scene.commands.push(PaintCommand::Fragment {
        scene: application,
        transform: Affine::translate((inset, inset)),
    });
    scene.commands.push(PaintCommand::PopLayer);
    scene.commands.push(PaintCommand::Stroke {
        style: kurbo::Stroke::new(scale.max(1.)),
        transform: Affine::IDENTITY,
        brush: Color::from_rgba8(90, 82, 104, 42).into(),
        brush_transform: None,
        shape: contour,
    });
    Color::TRANSPARENT
}
#[cfg(test)]
mod tests {
    use super::*;
    use plico_renderer::PaintScene;
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
                let options = plico_renderer::WindowOptions::new()
                    .decorations(true)
                    .corner_radius(16)
                    .initial_inner_size(160, 120)
                    .renderer(plico_renderer::RenderBackend::Wgpu);
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
    fn native_desktop_windows_delegate_the_outer_contour() {
        assert_eq!(
            client_corner_radius(16, false),
            if cfg!(any(target_os = "linux", target_os = "windows")) {
                0
            } else {
                16
            }
        );
        assert_eq!(client_corner_radius(0, true), 0);
        assert_eq!(
            client_corner_radius(16, true),
            if cfg!(any(target_os = "linux", target_os = "windows")) {
                0
            } else {
                16
            }
        );
    }
    #[test]
    fn linux_csd_wraps_content_in_a_continuous_shadowed_frame() {
        let mut scene = Scene::new();
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(20, 30, 40),
            None,
            &Rect::new(0., 0., 72., 52.),
        );
        let base =
            decorate_linux_client_scene(&mut scene, [100, 80], 14., 16., 1., Color::BLACK, true);
        assert_eq!(base, Color::TRANSPARENT);
        assert!(matches!(
            scene.commands.first(),
            Some(PaintCommand::BoxShadow {
                corner_smoothing: 1.,
                ..
            })
        ));
        assert!(scene.commands.iter().any(|command| matches!(
            command,
            PaintCommand::Fragment { transform, .. }
                if *transform == Affine::translate((14., 14.))
        )));
        assert!(matches!(
            scene.commands.last(),
            Some(PaintCommand::Stroke { .. })
        ));
    }

    #[test]
    fn linux_csd_frame_renders_a_transparent_margin_and_opaque_content() {
        let folder = tempfile::tempdir().unwrap();
        let mut scene = Scene::new();
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(30, 40, 50),
            None,
            &Rect::new(0., 0., 72., 52.),
        );
        let base =
            decorate_linux_client_scene(&mut scene, [100, 80], 14., 16., 1., Color::BLACK, true);
        let path = folder.path().join("linux-csd.png");
        crate::renderer::render_to_png(&scene, 100, 80, base, path.to_str().unwrap()).unwrap();
        let image = image::open(path).unwrap().into_rgba8();
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(image.get_pixel(50, 40)[3], 255);
        assert!(image.get_pixel(7, 40)[3] > 0);
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
