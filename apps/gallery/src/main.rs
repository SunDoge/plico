fn main() -> Result<(), plico_backend::DesktopError> {
    plico_backend::run_source_with_options(
        plico_backend::ZoomSource::new(Box::new(plico_backend::Source::new(
            plico_backend::NativeApp(plico_gallery::Gallery::default()),
        ))),
        plico_backend::WindowOptions::new()
            .title("Plico gallery")
            .initial_inner_size(1120, 820)
            .min_inner_size(640, 740),
    )
}
