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
            // it is covered by the separate image/host fixtures.
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
            "logo wrapped after zoom={zoom}: request={logo:?}, width={}, height={}, intrinsic={:?}", paragraph.metrics().width,paragraph.metrics().height,paragraph.metrics().intrinsic
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
