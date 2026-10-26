#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), plico_vello::DesktopError> {
    plico_vello::run_source_with_options(
        plico_vello::ZoomSource::new(Box::new({
            let app = gallery::Gallery::default();
            let widget =
                plico_renderer::image_surface::ImageSurface::new(app.images.surface.clone());
            plico_vello::SurfaceSource::new(
                gallery::images::GalleryHost { app },
                widget,
                "image-preview",
            )
        })),
        plico_vello::WindowOptions::new()
            .title("Plico gallery")
            .initial_inner_size(1120, 820)
            .min_inner_size(640, 740),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}
