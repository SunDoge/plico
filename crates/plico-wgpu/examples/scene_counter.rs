//! Real Plico declarations -> runtime/Taffy -> shared Scene -> wgpu, without Vello replay.
#[cfg(all(target_os = "linux", feature = "system-text", feature = "paint-scene"))]
mod demo {
    use plico::ImageAsset;
    use plico::prelude::*;
    use plico_renderer::{FrameSource, Scene, TextContext};
    use plico_runtime::{NativeView, Source};
    use plico_wgpu::Renderer;
    struct Counter {
        count: i32,
        image: ImageAsset,
    }
    impl plico::View for Counter {
        fn theme(&self) -> Theme {
            Theme::light()
        }
        fn show(&mut self, cx: &mut UiContext<'_>) {
            content(cx, &mut self.count, &self.image);
        }
    }
    #[component]
    #[rustfmt::skip]
    fn content(cx: &mut UiContext<'_>, count: &mut i32, asset: &ImageAsset) {
        column(cx).width_percent(100.).height_percent(100.).padding(32).gap(18).background(Color::Surface) + |cx| {
            row(cx).gap(14).align(Align::Center) + |cx| {
                avatar(cx).image(asset).size(48.);
                text(cx, "Plico / wgpu").font_size(26).weight(600);
                svg_source(cx, r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><circle cx="16" cy="16" r="14" fill="#dcfce7"/><path d="M9 16l5 5 10-11" fill="none" stroke="#15803d" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg>"##
                    .into()).size(32., 32.);
            };
            text(cx, "Immediate declarations · Taffy layout · system text").font_size(14).color(Color::Muted);
            column(cx).padding(24).gap(16).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
                text(cx, "A normal Plico component").font_size(18).weight(600);
                text!(cx, "Count: {}", count).font_size(32);
                text(cx, "中文与日本語の文字 · café · 👩‍💻").font_size(14);
                row(cx).gap(10) + |cx| {
                    if button(cx).text("Increment").primary().clicked() {
                        *count += 1;
                    }
                    if button(cx).text("Reset").ghost().clicked() {
                        *count = 0;
                    }
                };
            };
        };
    }
    fn sample_image() -> Result<ImageAsset, Box<dyn std::error::Error>> {
        use image::ImageEncoder;
        let pixels = image::RgbaImage::from_fn(96, 96, |x, y| {
            let ring = ((x as f32 - 48.).hypot(y as f32 - 48.) / 12.) as u32 % 2;
            image::Rgba([
                30 + (x * 2) as u8,
                80 + (y / 2) as u8,
                if ring == 0 { 220 } else { 150 },
                255,
            ])
        });
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            pixels.as_raw(),
            96,
            96,
            image::ExtendedColorType::Rgba8,
        )?;
        Ok(ImageAsset::from_bytes(&png))
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let output = std::path::Path::new("reports/wgpu-scene");
        std::fs::create_dir_all(output)?;
        let mut renderer = pollster::block_on(Renderer::new())?;
        for scale in [1., 1.25, 1.5, 2.] {
            let mut text = TextContext::new();
            renderer.configure_text(&mut text, scale)?;
            let mut source = Source::with_clock(
                NativeView(Counter {
                    count: 0,
                    image: sample_image()?,
                }),
                plico::VirtualClock::default(),
            )
            .with_font_family("Noto Sans, Noto Sans CJK SC, sans-serif");
            for count in [0, 42] {
                source.update(|v| v.0.count = count);
                let mut scene = Scene::new();
                source.build_and_paint(&mut scene, &mut text, 620, 410, scale);
                if let Some(error) = &source.last_error {
                    return Err(format!("source failed: {error}").into());
                }
                if let Some(error) = text.take_native_paragraph_error() {
                    return Err(error.into());
                }
                let size = [(620. * scale) as u32, (410. * scale) as u32];
                let prepared = renderer.prepare_scene(&scene, size)?;
                let image = renderer.render_items(
                    &prepared.draws(),
                    size,
                    source.base_color().components,
                    0.,
                )?;
                image::save_buffer(
                    output.join(format!("counter-{count}-{scale}x.png")),
                    &image,
                    size[0],
                    size[1],
                    image::ColorType::Rgba8,
                )?;
                println!(
                    "scale={scale} count={count} paint commands={} backend={} native shape cache={:?}",
                    scene.flattened().commands.len(),
                    text.raster_backend_name(),
                    text.cache_entries()
                );
            }
        }
        Ok(())
    }
}
#[cfg(all(target_os = "linux", feature = "system-text", feature = "paint-scene"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    demo::run()
}
#[cfg(not(all(target_os = "linux", feature = "system-text", feature = "paint-scene")))]
fn main() {
    eprintln!("This shared-scene example currently requires Linux, system-text and paint-scene.");
}
