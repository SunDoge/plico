//! One settings application, with compile-time backend capabilities.
#[cfg(any(feature = "fltk", feature = "wgpu"))]
use snafu::ResultExt;
use snafu::Snafu;

#[derive(usage_rs::Cli)]
#[usage(
    bin = "backend-settings",
    about = "Shared settings UI for wgpu, NSH and FLTK"
)]
struct Options {
    /// Renderer to use; defaults to the first enabled: wgpu, nsh, fltk.
    #[usage(long)]
    backend: Option<String>,
}

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("enable a backend with --features wgpu, nsh or fltk"))]
    NoBackend,
    #[snafu(display(
        "backend {backend:?} is unavailable; enable its Cargo feature (choose wgpu, nsh or fltk)"
    ))]
    Backend { backend: String },
    #[cfg(feature = "wgpu")]
    #[snafu(display("wgpu window failed: {source}"))]
    Wgpu { source: plico_desktop::DesktopError },
    #[cfg(feature = "fltk")]
    #[snafu(display("FLTK window failed: {source}"))]
    Fltk { source: plico_fltk::Error },
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("backend-settings: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<std::process::ExitCode, Error> {
    let options = Options::parse();
    let backend = options
        .backend
        .as_deref()
        .or_else(|| {
            [
                (cfg!(feature = "wgpu"), "wgpu"),
                (cfg!(feature = "nsh"), "nsh"),
                (cfg!(feature = "fltk"), "fltk"),
            ]
            .into_iter()
            .find_map(|(enabled, name)| enabled.then_some(name))
        })
        .ok_or(Error::NoBackend)?;
    match backend {
        #[cfg(feature = "wgpu")]
        "wgpu" => {
            let subscriber = tracing_subscriber::fmt().with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            );
            #[cfg(feature = "profiling")]
            let subscriber =
                subscriber.with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE);
            let _ = subscriber.try_init();
            plico_desktop::run_source_with_options(
                plico_desktop::Source::new(backend_settings::ImmediateSettings::default()),
                plico_desktop::WindowOptions::new()
                    .title("Plico settings — wgpu")
                    .initial_inner_size(1100, 820)
                    .renderer(plico_renderer::RenderBackend::Wgpu),
            )
            .context(WgpuSnafu)?;
            Ok(std::process::ExitCode::SUCCESS)
        }
        #[cfg(feature = "nsh")]
        "nsh" => {
            use plico_nsh::native::{platform, root_with_viewport};
            Ok(platform::run_app(
                platform::AppSource::Rust(root_with_viewport(
                    backend_settings::ImmediateSettings::default,
                    1100.,
                    820.,
                )),
                platform::AppOptions {
                    app_id: "dev.plico.backend-settings".to_owned(),
                    title: "Plico settings — NSH".to_owned(),
                    size: (1100, 820),
                    ..Default::default()
                },
            )
            .into())
        }
        #[cfg(feature = "fltk")]
        "fltk" => {
            plico_fltk::run(
                backend_settings::ImmediateSettings::default(),
                "Plico settings — FLTK",
                1100,
                820,
            )
            .context(FltkSnafu)?;
            Ok(std::process::ExitCode::SUCCESS)
        }
        backend => Err(Error::Backend {
            backend: backend.to_owned(),
        }),
    }
}
