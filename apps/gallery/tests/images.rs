#![cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
use gallery::{Gallery, Page};
use plico_core::Router;
use plico_renderer::{
    animated_image::{AnimatedImage, AnimationLimits},
    image_surface::ImageLoadStatus,
};
use plico_vello::{
    NativeApp,
    testing::{Command, Driver},
};
use std::sync::Arc;
fn click(label: &str) -> Command {
    Command::Click {
        role: "button".into(),
        label: label.into(),
        occurrence: None,
    }
}
#[test]
fn image_controls_work_without_network_or_manual_rebuild_requests() {
    let app = Gallery {
        router: Router::new(Page::Images),
        ..Default::default()
    };
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        encoder
            .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                2,
                2,
                image::Rgba([255, 0, 0, 255]),
            )))
            .unwrap();
    }
    let ticket = app.images.surface.begin_loading();
    app.images.surface.complete(
        ticket,
        Ok(Arc::new(
            AnimatedImage::decode_gif(&bytes, AnimationLimits::default()).unwrap(),
        )),
    );
    let surface = app.images.surface.clone();
    let mut driver = Driver::new(NativeApp(app), 1120, 820).unwrap();
    assert!(matches!(surface.status(), ImageLoadStatus::Ready { .. }));
    driver.execute(click("Pause")).unwrap();
    assert!(surface.paused());
    driver.execute(click("Play")).unwrap();
    assert!(!surface.paused());
    driver.execute(click("Replay")).unwrap();
    assert!(!surface.paused());
    driver.execute(click("GIF")).unwrap();
    assert_eq!(surface.status(), ImageLoadStatus::Loading);
    assert!(
        driver
            .snapshot()
            .nodes
            .iter()
            .any(|n| n.label.as_deref() == Some("https://www.gstatic.com/webp/animated/1.gif"))
    );
    driver.execute(click("WebP")).unwrap();
    assert!(
        driver
            .snapshot()
            .nodes
            .iter()
            .any(|n| n.label.as_deref() == Some("https://www.gstatic.com/webp/animated/1.webp"))
    );
}

#[test]
#[ignore = "downloads public Google samples; normal CI stays offline"]
fn desktop_host_downloads_decodes_and_schedules_both_formats() {
    use gallery::images::{Format, GalleryHost};
    use plico_renderer::{FrameSource, Scene, TextContext, image_surface::ImageSurface};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    for format in [Format::WebP, Format::Gif] {
        let app = Gallery {
            router: Router::new(Page::Images),
            ..Default::default()
        };
        let state = app.images.surface.clone();
        app.images.request(format, true);
        let clock = plico_core::VirtualClock::default();
        let widget = ImageSurface::with_clock(state.clone(), Arc::new(clock.clone()));
        let mut source =
            plico_vello::SurfaceSource::new(GalleryHost { app }, widget, "image-preview");
        let (tx, rx) = mpsc::channel();
        source.set_wake_callback(Arc::new(move || {
            let _ = tx.send(());
        }));
        let mut text = TextContext::new();
        source.build_frame(&mut text, 1120, 820);
        source.poll_async();
        let deadline = Instant::now() + Duration::from_secs(30);
        while matches!(state.status(), ImageLoadStatus::Loading) {
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("download completion wake");
            source.poll_async();
        }
        assert!(
            matches!(state.status(),ImageLoadStatus::Ready{frames,..} if frames>1),
            "{:?}",
            state.status()
        );
        let nodes = source.build_frame(&mut text, 1120, 820);
        assert!(source.source.last_error.is_none());
        let mut scene = Scene::new();
        source.paint_scene(&mut scene, &nodes, &mut text, (1120, 820), 1.);
        assert!(source.animation_deadline().is_some());
        state.set_paused(true);
        source.poll_async();
        source.paint_scene(&mut scene, &nodes, &mut text, (1120, 820), 1.);
        assert!(source.animation_deadline().is_none());
    }
}
