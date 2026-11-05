#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(usage_rs::Cli)]
    struct Options {
        #[usage(long)]
        scenario: Option<std::path::PathBuf>,
        #[usage(long)]
        capture: Option<std::path::PathBuf>,
    }
    let options = Options::parse();
    #[cfg(not(any(feature = "desktop", feature = "wgpu")))]
    if options.capture.is_some() || options.scenario.is_some() {
        return Err("scenario capture requires the gallery desktop (Skia) or wgpu feature".into());
    }
    #[cfg(any(feature = "desktop", feature = "wgpu"))]
    if let Some(output) = options.capture {
        let scenario = if let Some(path) = options.scenario {
            plico_desktop::PreviewScenario::<gallery::Gallery>::load(path)?
        } else {
            Default::default()
        };
        scenario
            .render(plico_desktop::NativeApp)?
            .write_png(output)?;
        return Ok(());
    }
    plico_desktop::run_source_with_options(
        plico_desktop::ZoomSource::new(Box::new({
            let app = gallery::Gallery::default();
            let widget =
                plico_renderer::image_surface::ImageSurface::new(app.images.surface.clone());
            plico_desktop::SurfaceSource::new(
                gallery::images::GalleryHost { app },
                widget,
                "image-preview",
            )
        })),
        plico_desktop::WindowOptions::new()
            .title("Plico gallery")
            .initial_inner_size(1120, 820)
            .min_inner_size(640, 740),
    )?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
