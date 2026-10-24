fn main() -> Result<(), plico_vello::DesktopError> {
    plico_vello::run_source_with_options(
        plico_vello::Source::new(plico_backend_settings::Settings::default()),
        plico_vello::WindowOptions::new()
            .title("Plico settings — Vello")
            .initial_inner_size(1100, 820),
    )
}
