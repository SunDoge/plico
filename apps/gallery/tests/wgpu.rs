//! Opt-in real application coverage; no display server is needed.
#![cfg(all(feature = "wgpu", target_os = "linux"))]
use gallery::{Gallery, Page, i18n::Language};
use plico::{Route, Router, VirtualClock};
use plico_desktop::Source;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_wgpu::Renderer;
use std::time::Duration;

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn gallery_pages_native_text_and_damage_match_full_replay() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut failures = Vec::new();
    for language in [Language::English, Language::Chinese] {
        for &page in Page::ALL {
            // This page performs live network IO and hosts an image surface;
            // the deterministic image-surface fixture below covers its host path.
            if page == Page::Images {
                continue;
            }
            let clock = VirtualClock::default();
            let mut source = Source::with_clock(
                Gallery {
                    language,
                    router: Router::new(page),
                    ..Default::default()
                },
                clock.clone(),
            );
            let mut text = TextContext::new();
            partial.configure_text(&mut text, 1.).unwrap();
            let mut scene = Scene::new();
            for delta in [0, 200, 2000, 15000] {
                clock.advance(Duration::from_millis(delta)).unwrap();
                source
                    .render_retained_frame(&mut scene, &mut text, (1120, 820), 1.)
                    .unwrap();
                if let Some(error) = &source.last_error {
                    failures.push(format!("{language:?}/{page:?}: {error}"));
                    break;
                }
                if let Some(error) = text.take_native_paragraph_error() {
                    failures.push(format!("{language:?}/{page:?}: {error}"));
                    break;
                }
                let result = partial.render_scene(&scene, [1120, 820], [1.; 4], true);
                match result {
                    Ok(actual) => {
                        let expected = full
                            .render_scene(&scene, [1120, 820], [1.; 4], false)
                            .unwrap();
                        assert_eq!(actual, expected, "{language:?}/{page:?} delta={delta}");
                        assert_eq!(text.cache_entries(), (0, 0), "unexpected Parley fallback");
                    }
                    Err(error) => {
                        failures.push(format!("{language:?}/{page:?}: {error}"));
                        break;
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "gallery coverage failures:\n{}",
        failures.join("\n")
    );
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn zoomed_surface_host_measures_and_paints_native_text_at_the_same_scale() {
    use plico_desktop::{SurfaceSource, ZoomSource};
    use plico_renderer::PaintCommand;
    let app = Gallery::default();
    let widget = plico_renderer::image_surface::ImageSurface::new(app.images.surface.clone());
    let mut source = ZoomSource::new(Box::new(SurfaceSource::new(
        gallery::images::GalleryHost { app },
        widget,
        "image-preview",
    )));
    let controller = source.controller();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut oracle = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    for zoom in [1., 1.25, 1.5, 2., 1.] {
        assert!(controller.set(zoom));
        source.set_device_scale(1.);
        let mut scene = Scene::new();
        assert!(
            source
                .render_retained_frame(&mut scene, &mut text, (1120, 820), 1.)
                .is_none()
        );
        source.build_and_paint(&mut scene, &mut text, 1120, 820, 1.);
        assert!(
            !text.set_native_paragraph_scale(zoom),
            "measurement scale was not synchronized"
        );
        let generation = text.cache_generation();
        let mut geometry = TextContext::new();
        oracle.configure_text(&mut geometry, zoom).unwrap();
        let flattened = scene.flattened();
        let logo = flattened
            .commands
            .iter()
            .find_map(|command| match command {
                PaintCommand::Paragraph { request, .. } if request.text.as_ref() == "plico" => {
                    Some(request.clone())
                }
                _ => None,
            })
            .unwrap();
        let paragraph = geometry.native_paragraph(logo.clone()).unwrap();
        assert!(
            paragraph.metrics().height < logo.font_size * 1.8,
            "logo wrapped after zoom={zoom}: request={logo:?}, width={}, height={}, intrinsic={:?}",
            paragraph.metrics().width,
            paragraph.metrics().height,
            paragraph.metrics().intrinsic
        );
        let expected = oracle
            .render_scene(&scene, [1120, 820], [1.; 4], false)
            .unwrap();
        let actual = renderer
            .render_scene(&scene, [1120, 820], [1.; 4], true)
            .unwrap();
        assert_eq!(actual, expected);
        source.build_and_paint(&mut scene, &mut text, 1120, 820, 1.);
        assert_eq!(
            generation,
            text.cache_generation(),
            "unchanged zoom flushed measurement caches"
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn animated_image_surface_plays_pauses_hides_and_reuses_uploaded_frames() {
    use plico_desktop::SurfaceSource;
    use plico_renderer::{
        animated_image::{AnimatedImage, AnimationLimits},
        image_surface::ImageSurface,
    };
    use std::sync::Arc;
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        encoder
            .set_repeat(image::codecs::gif::Repeat::Infinite)
            .unwrap();
        for rgba in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(32, 16, image::Rgba(rgba)),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
    }
    let resource = Arc::new(AnimatedImage::decode_gif(&bytes, AnimationLimits::default()).unwrap());
    assert_eq!(resource.frame_count(), 2);
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for scale in [1., 1.25, 2.] {
        let app = Gallery {
            router: Router::new(Page::Images),
            ..Default::default()
        };
        let state = app.images.surface.clone();
        let ticket = state.begin_loading();
        assert!(state.complete(ticket, Ok(resource.clone())));
        let clock = VirtualClock::default();
        let widget = ImageSurface::with_clock(state.clone(), Arc::new(clock.clone()));
        let mut source = SurfaceSource::with_clock(
            gallery::images::GalleryHost { app },
            widget,
            "image-preview",
            clock.clone(),
        );
        let mut text = TextContext::new();
        partial.configure_text(&mut text, scale).unwrap();
        source.set_device_scale(scale);
        let size = [(1120. * scale) as u32, (820. * scale) as u32];
        let mut scene = Scene::new();
        let mut frame = |source: &mut SurfaceSource<gallery::images::GalleryHost, ImageSurface>,
                         partial: &mut Renderer| {
            source.poll_async();
            source.build_and_paint(&mut scene, &mut text, 1120, 820, scale);
            assert!(source.source.last_error.is_none());
            assert!(text.take_native_paragraph_error().is_none());
            let actual = partial.render_scene(&scene, size, [1.; 4], true).unwrap();
            let expected = full.render_scene(&scene, size, [1.; 4], false).unwrap();
            assert_eq!(actual, expected, "animated image damage scale={scale}");
            actual
        };
        let red = frame(&mut source, &mut partial);
        assert!(source.animation_deadline().is_some());
        let count_color = |pixels: &[u8], rgba: [u8; 4]| {
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == rgba)
                .count()
        };
        assert!(
            count_color(&red, [255, 0, 0, 255]) > 100_000,
            "image surface did not draw red"
        );
        clock.advance(Duration::from_millis(120)).unwrap();
        let blue = frame(&mut source, &mut partial);
        assert!(
            count_color(&blue, [0, 0, 255, 255]) > 100_000,
            "animation did not advance"
        );
        let uploads = partial.image_cache_stats().2;
        for _ in 0..16 {
            clock.advance(Duration::from_millis(120)).unwrap();
            frame(&mut source, &mut partial);
        }
        assert_eq!(
            partial.image_cache_stats().2,
            uploads,
            "warm animation uploaded textures"
        );
        state.set_paused(true);
        let paused = frame(&mut source, &mut partial);
        assert!(source.animation_deadline().is_none());
        clock.advance(Duration::from_secs(2)).unwrap();
        assert_eq!(
            frame(&mut source, &mut partial),
            paused,
            "paused image changed"
        );
        assert_eq!(
            partial.scene_damage_coverage(size),
            0.,
            "paused surface kept damaging pixels"
        );
        state.set_paused(false);
        frame(&mut source, &mut partial);
        assert!(source.animation_deadline().is_some());
        source
            .source
            .update(|view| view.app.router.go(Page::Workspace));
        let hidden = frame(&mut source, &mut partial);
        assert!(source.animation_deadline().is_none());
        assert!(
            count_color(&hidden, [255, 0, 0, 255]) < 1000
                && count_color(&hidden, [0, 0, 255, 255]) < 1000,
            "unmounted image left stale pixels"
        );
        let pending = state.begin_loading();
        assert!(
            !state.complete(ticket, Ok(resource.clone())),
            "obsolete load replaced current state"
        );
        assert!(matches!(
            state.status(),
            plico_renderer::image_surface::ImageLoadStatus::Loading
        ));
        assert!(state.complete(pending, Err("Deterministic image load failure".into())));
        source
            .source
            .update(|view| view.app.router.go(Page::Images));
        let failed = frame(&mut source, &mut partial);
        assert!(source.animation_deadline().is_none());
        assert!(
            count_color(&failed, [255, 0, 0, 255]) < 1000
                && count_color(&failed, [0, 0, 255, 255]) < 1000,
            "failed load painted an old frame"
        );
        assert!(
            source
                .source
                .adapter
                .placed
                .iter()
                .any(|p| p.paint.text.as_deref() == Some("Deterministic image load failure"))
        );
        let retry = state.begin_loading();
        assert!(state.complete(retry, Ok(resource.clone())));
        let recovered = frame(&mut source, &mut partial);
        assert!(
            count_color(&recovered, [255, 0, 0, 255]) > 100_000,
            "retry did not restart playback"
        );
        assert!(source.animation_deadline().is_some());
    }
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn mines_reveal_reuses_native_paragraphs_and_partial_pixels_match_full() {
    let clock = VirtualClock::default();
    let mut game = gallery::mines::Mines::default();
    game.configure(16, 16, 40);
    game.reveal(8 * 16 + 8);
    let mut source = Source::with_clock(
        Gallery {
            router: Router::new(Page::Mines),
            mines: game,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    let mut scene = Scene::new();
    clock.set(Duration::from_millis(300)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text, (1120, 820), 1.)
        .unwrap();
    renderer
        .render_scene(&scene, [1120, 820], [1.; 4], true)
        .unwrap();
    let mut misses = None;
    for ms in [350, 367, 400, 450, 500, 600] {
        clock.set(Duration::from_millis(ms)).unwrap();
        source
            .render_retained_frame(&mut scene, &mut text, (1120, 820), 1.)
            .unwrap();
        assert!(source.last_error.is_none());
        let actual = renderer
            .render_scene(&scene, [1120, 820], [1.; 4], true)
            .unwrap();
        let expected = full
            .render_scene(&scene, [1120, 820], [1.; 4], false)
            .unwrap();
        assert_eq!(actual, expected, "partial replay at {ms} ms");
        let current = renderer.paragraph_cache_stats().misses;
        if let Some(misses) = misses {
            assert_eq!(current, misses, "animation reshaped native text at {ms} ms");
        }
        misses = Some(current);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn retained_scale_motion_shapes_native_text_once_and_keeps_static_quality() {
    struct Demo {
        scale: f32,
        shows: usize,
    }
    impl Default for Demo {
        fn default() -> Self {
            Self {
                scale: 1.,
                shows: 0,
            }
        }
    }
    impl plico_dev::App for Demo {
        #[plico::component]
        fn show(&mut self, cx: &mut plico::UiContext) {
            self.shows += 1;
            plico::widgets::column(cx)
                .width(260.)
                .height(100.)
                .padding(24)
                .scale(self.scale, self.scale)
                .transition(240)
                .children(|cx| {
                    plico::widgets::text(cx, "Native 123 中文")
                        .font_size(24)
                        .weight(600);
                });
        }
    }
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(plico_desktop::NativeApp(Demo::default()), clock.clone());
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    renderer
        .render_scene(&scene, [400, 200], [1.; 4], false)
        .unwrap();
    source.update(|view| view.0.scale = 1.2);
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    renderer
        .render_scene(&scene, [400, 200], [1.; 4], false)
        .unwrap();
    let shows = source.view.0.shows;
    let mut misses = None;
    for ms in [16, 32, 64, 100, 160, 220] {
        clock.set(Duration::from_millis(ms)).unwrap();
        source
            .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
            .unwrap();
        renderer
            .render_scene(&scene, [400, 200], [1.; 4], false)
            .unwrap();
        assert_eq!(
            source.view.0.shows, shows,
            "motion re-evaluated the application"
        );
        assert_eq!(source.adapter.stats.layout_runs, 0);
        let current = renderer.paragraph_cache_stats().misses;
        if let Some(misses) = misses {
            assert_eq!(current, misses, "scale motion reshaped text at {ms} ms");
        }
        misses = Some(current);
    }
    clock.set(Duration::from_millis(260)).unwrap();
    source
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    let animated = renderer
        .render_scene(&scene, [400, 200], [1.; 4], false)
        .unwrap();
    let mut baseline = Source::new(plico_desktop::NativeApp(Demo {
        scale: 1.2,
        shows: 0,
    }));
    let mut direct = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    direct.configure_text(&mut text, 1.).unwrap();
    let mut scene = Scene::new();
    baseline
        .render_retained_frame(&mut scene, &mut text, (400, 200), 1.)
        .unwrap();
    let expected = direct
        .render_scene(&scene, [400, 200], [1.; 4], false)
        .unwrap();
    assert_eq!(
        animated, expected,
        "settled text differs from exact native rasterization"
    );
    assert!(!source.has_anim());
}
