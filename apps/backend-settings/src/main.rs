//! One settings application, with compile-time backend capabilities.
#[cfg(any(
    feature = "fltk",
    feature = "vello",
    feature = "skia",
    feature = "egui"
))]
use snafu::ResultExt;
use snafu::Snafu;

#[derive(usage_rs::Cli)]
#[usage(
    bin = "backend-settings",
    about = "Shared settings UI for Vello, Skia, egui and FLTK"
)]
struct Options {
    /// Renderer to use; defaults to the first enabled backend: vello, skia, fltk.
    #[usage(long)]
    backend: Option<String>,
}

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("enable a backend with --features vello, skia or fltk"))]
    NoBackend,
    #[snafu(display(
        "backend {backend:?} is unavailable; enable its Cargo feature (choose vello, skia or fltk)"
    ))]
    Backend { backend: String },
    #[cfg(feature = "vello")]
    #[snafu(display("Vello window failed: {source}"))]
    Vello { source: plico_desktop::DesktopError },
    #[cfg(feature = "skia")]
    #[snafu(display("Skia window failed: {source}"))]
    Skia { source: plico_shell::Error },
    #[cfg(feature = "fltk")]
    #[snafu(display("FLTK window failed: {source}"))]
    Fltk { source: plico_fltk::Error },
    #[cfg(feature = "egui")]
    #[snafu(display("egui window failed: {source}"))]
    Egui { source: plico_shell::Error },
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("backend-settings: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Error> {
    let options = Options::parse();
    let backend = options
        .backend
        .as_deref()
        .or_else(|| {
            [
                (cfg!(feature = "vello"), "vello"),
                (cfg!(feature = "skia"), "skia"),
                (cfg!(feature = "egui"), "egui"),
                (cfg!(feature = "fltk"), "fltk"),
            ]
            .into_iter()
            .find_map(|(enabled, name)| enabled.then_some(name))
        })
        .ok_or(Error::NoBackend)?;
    match backend {
        #[cfg(feature = "vello")]
        "vello" => plico_desktop::run_source_with_options(
            plico_desktop::Source::new(backend_settings::Settings::default()),
            plico_desktop::WindowOptions::new()
                .title("Plico settings — Vello")
                .initial_inner_size(1100, 820),
        )
        .context(VelloSnafu),
        #[cfg(feature = "skia")]
        "skia" => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
            plico_shell::run_window_with_options(
                Box::new(plico_runtime::Source::new(
                    backend_settings::Settings::default(),
                )),
                plico_renderer::WindowOptions::new()
                    .title("Plico settings — Skia")
                    .initial_inner_size(1100, 820)
                    .renderer(plico_renderer::RenderBackend::Skia),
            )
            .context(SkiaSnafu)
        }
        #[cfg(feature = "egui")]
        "egui" => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
            plico_shell::run_window_with_options(
                Box::new(plico_egui::Source::new(
                    backend_settings::Settings::default(),
                )),
                plico_renderer::WindowOptions::new()
                    .title("Plico settings — egui")
                    .initial_inner_size(1100, 820)
                    .renderer(plico_renderer::RenderBackend::Egui),
            )
            .context(EguiSnafu)
        }
        #[cfg(feature = "fltk")]
        "fltk" => plico_fltk::run(
            backend_settings::Settings::default(),
            "Plico settings — FLTK",
            1100,
            820,
        )
        .context(FltkSnafu),
        backend => Err(Error::Backend {
            backend: backend.to_owned(),
        }),
    }
}
