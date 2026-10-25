fn main() -> Result<(), plico_vello::DesktopError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    plico_vello::run_source_with_options(
        plico_vello::Source::new(plico_backend_settings::Settings::default()),
        plico_vello::WindowOptions::new()
            .title("Plico settings — Skia")
            .initial_inner_size(1100, 820)
            .renderer(plico_vello::RenderBackend::Skia),
    )
}
