//! Synthetic projection capture; no background task or live data.
use plico_dev::services::HostService;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_vello::{ServiceView, Source};
use services_demo::{Application, TaskCommand, TaskSnapshot, TaskStatus};
use snafu::{ResultExt, Snafu};
use std::path::PathBuf;
#[derive(usage_rs::Cli)]
#[usage(
    bin = "services-capture",
    about = "Capture the task workspace with synthetic data"
)]
struct Options {
    #[usage(long)]
    out: PathBuf,
    #[usage(long)]
    running: bool,
}
struct Projection(TaskSnapshot);
impl HostService for Projection {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;
    fn snapshot(&self) -> TaskSnapshot {
        self.0.clone()
    }
    fn send(&mut self, _command: TaskCommand) {}
}
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("capture IO failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("capture failed: {source}"))]
    Render { source: plico_shell::Error },
    #[snafu(display("output path must be UTF-8"))]
    Path,
}
fn run() -> Result<(), Error> {
    let options = Options::parse();
    if let Some(parent) = options.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).context(IoSnafu)?;
    }
    let snapshot = TaskSnapshot {
        progress: if options.running { 54 } else { 0 },
        status: if options.running {
            TaskStatus::Running
        } else {
            TaskStatus::Idle
        },
        ..Default::default()
    };
    let mut source = Source::with_clock(
        ServiceView::new(Application::default(), Projection(snapshot)),
        plico_core::VirtualClock::default(),
    );
    let mut tcx = TextContext::new();
    let nodes = source.build_frame(&mut tcx, 620, 520);
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, &mut tcx, (620, 520), 1.);
    plico_shell::renderer::render_to_png(
        &scene,
        620,
        520,
        source.base_color(),
        options.out.to_str().ok_or(Error::Path)?,
    )
    .context(RenderSnafu)
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
