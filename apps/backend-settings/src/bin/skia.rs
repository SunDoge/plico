fn main() -> Result<(), plico_shell::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    plico_shell::run_window_with_options(
        Box::new(plico_runtime::Source::new(
            plico_backend_settings::Settings::default(),
        )),
        plico_renderer::WindowOptions::new()
            .title("Plico settings — Skia")
            .initial_inner_size(1100, 820)
            .renderer(plico_renderer::RenderBackend::Skia),
    )
}
