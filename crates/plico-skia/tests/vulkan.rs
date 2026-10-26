#![cfg(feature = "vulkan")]
use plico_renderer::{PaintScene, Scene};
use plico_skia::{Error, SkiaGpuSurface};
use vello_common::{
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
};

#[test]
fn invalid_sizes_do_not_initialize_vulkan() {
    assert!(matches!(
        SkiaGpuSurface::new(0, 16),
        Err(Error::InvalidSize { .. })
    ));
    assert!(matches!(
        SkiaGpuSurface::new(16, u32::MAX),
        Err(Error::InvalidSize { .. })
    ));
}

#[test]
#[ignore = "requires a Vulkan 1.1 device; run explicitly on hardware or lavapipe"]
fn submission_readback_resize_and_destruction() {
    for _ in 0..3 {
        let mut gpu = match std::env::var("PLICO_SKIA_DEVICE") {
            Ok(name) => SkiaGpuSurface::with_device(16, 16, &name),
            Err(_) => SkiaGpuSurface::new(16, 16),
        }
        .unwrap();
        println!(
            "Vulkan: {} (software: {})",
            gpu.device_name(),
            gpu.is_software_device()
        );
        assert!(matches!(gpu.read_pixels(), Err(Error::NoFrame)));
        let mut scene = Scene::new();
        scene.push_clip_layer(Affine::translate((4., 4.)), &Rect::new(0., 0., 4., 4.));
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(200, 100, 50, 128),
            None,
            &Rect::new(0., 0., 16., 16.),
        );
        scene.pop_layer();
        // Queue multiple frames without screenshot readback in between.
        for _ in 0..20 {
            gpu.draw_scene(&scene, Color::TRANSPARENT).unwrap();
        }
        let bytes = gpu.read_pixels().unwrap();
        assert_eq!(&bytes[..4], [0; 4]);
        let offset = (5 * 16 + 5) * 4;
        for (actual, expected) in bytes[offset..offset + 4].iter().zip([100u8, 50, 25, 128]) {
            assert!(actual.abs_diff(expected) <= 1, "{actual} != {expected}");
        }
        assert!(gpu.resize(0, 32).is_err());
        assert_eq!(gpu.dimensions(), (16, 16));
        assert_eq!(gpu.read_pixels().unwrap(), bytes);
        gpu.resize(32, 24).unwrap();
        assert_eq!(gpu.dimensions(), (32, 24));
        assert!(matches!(gpu.read_pixels(), Err(Error::NoFrame)));
        let mut unsupported = Scene::new();
        unsupported.push_layer(
            vello_common::peniko::BlendMode::default(),
            1.,
            Affine::IDENTITY,
            &Rect::new(0., 0., 4., 4.),
            Some(std::sync::Arc::new(
                vello_common::filter_effects::Filter::from_function(
                    vello_common::filter_effects::FilterFunction::Blur { radius: 1. },
                ),
            )),
            None,
        );
        assert!(matches!(
            gpu.draw_scene(&unsupported, Color::WHITE),
            Err(Error::Unsupported { .. })
        ));
        assert!(matches!(gpu.read_pixels(), Err(Error::NoFrame)));
        let bytes = gpu.render_scene(&Scene::new(), Color::WHITE).unwrap();
        assert_eq!(bytes.len(), 32 * 24 * 4);
        assert!(bytes.iter().all(|v| *v == 255));
        // Drop must wait for submitted work before releasing Vulkan handles.
    }
}

#[test]
#[ignore = "requires the Vulkan loader to enumerate devices"]
fn missing_device_is_an_error_instead_of_cpu_fallback() {
    assert!(matches!(
        SkiaGpuSurface::with_device(16, 16, "plico nonexistent test GPU"),
        Err(Error::Gpu { .. })
    ));
}

#[test]
#[ignore = "requires a Vulkan device; validates the image path used by music covers"]
fn repeated_image_frames_keep_texture_and_background() {
    use plico_renderer::image::RasterImage;
    let mut gpu = SkiaGpuSurface::new(64, 64).unwrap();
    let image = RasterImage::from_rgba(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([240, 80, 40, 255]),
    ));
    let mut scene = Scene::new();
    scene.draw_image(
        image.brush().into(),
        Affine::translate((8., 8.)) * Affine::scale(4.),
    );
    for frame in 0..30 {
        gpu.draw_scene(&scene, Color::from_rgb8(30, 32, 48))
            .unwrap();
        if frame % 5 == 0 {
            let bytes = gpu.read_pixels().unwrap();
            assert_eq!(
                &bytes[..4],
                &[30, 32, 48, 255],
                "background at frame {frame}"
            );
            let offset = (16 * 64 + 16) * 4;
            assert_eq!(
                &bytes[offset..offset + 4],
                &[240, 80, 40, 255],
                "image at frame {frame}"
            );
        }
    }
}

#[test]
#[ignore = "requires Vulkan; compares partial GPU drawing against full GPU drawing"]
fn damage_tracked_gpu_matches_full_redraw_and_resize() {
    let make = || {
        match std::env::var("PLICO_SKIA_DEVICE") {
            Ok(name) => SkiaGpuSurface::with_device(32, 32, &name),
            Err(_) => SkiaGpuSurface::new(32, 32),
        }
        .unwrap()
    };
    let mut partial = make();
    let mut full = make();
    println!("damage comparison device={}", partial.device_name());
    for frame in 0..60 {
        if frame == 30 {
            partial.resize(40, 36).unwrap();
            full.resize(40, 36).unwrap();
        }
        let mut scene = Scene::new();
        if frame % 11 != 0 {
            scene.fill(
                Fill::NonZero,
                Affine::translate(((frame % 20) as f64, 0.)),
                Color::from_rgba8(240, 80, 40, 128),
                None,
                &Rect::new(0., 4., 7., 20.),
            );
        }
        partial.draw_scene_tracked(&scene, Color::WHITE).unwrap();
        full.draw_scene(&scene, Color::WHITE).unwrap();
        assert_eq!(
            partial.read_pixels().unwrap(),
            full.read_pixels().unwrap(),
            "frame {frame}"
        );
    }
}
