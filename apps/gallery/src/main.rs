fn main() -> Result<(), plico_vello::DesktopError> {
    plico_vello::run_source_with_options(
        plico_vello::ZoomSource::new(Box::new(plico_vello::Source::new(plico_vello::NativeApp(
            gallery::Gallery::default(),
        )))),
        plico_vello::WindowOptions::new()
            .title("Plico gallery")
            .initial_inner_size(1120, 820)
            .min_inner_size(640, 740),
    )
}
