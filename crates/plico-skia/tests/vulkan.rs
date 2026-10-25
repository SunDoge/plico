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
