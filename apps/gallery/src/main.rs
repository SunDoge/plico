#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), plico_desktop::DesktopError> {
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
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}
