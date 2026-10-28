use launcher::Launcher;
use plico_desktop::Source;
use plico_renderer::{FrameSource, Scene, TextContext};
use snafu::{ResultExt, Snafu};
use std::path::PathBuf;
#[derive(usage_rs::Cli)]
#[usage(bin = "launcher-capture", about = "Capture the launcher demo")]
struct Options {
    #[usage(long)]
    out: PathBuf,
    #[usage(long)]
    light: bool,
    #[usage(long, default = "")]
    query: String,
    #[usage(long, default = "680")]
    width: u32,
}
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("capture failed: {source}"))]
    Render { source: plico_shell::Error },
    #[snafu(display("capture IO failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("{message}"))]
    Input { message: &'static str },
}
fn run() -> Result<(), Error> {
    let options = Options::parse();
    snafu::ensure!(
        (400..=1600).contains(&options.width),
        InputSnafu {
            message: "width must be between 400 and 1600"
        }
    );
    if let Some(parent) = options.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).context(IoSnafu)?;
    }
    let mut app = Launcher::default();
    app.dark = !options.light;
    app.query
        .edit(plico::InputEdit::Insert(options.query.into()));
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(app, clock);
    let mut tcx = TextContext::new();
    let nodes = source.build_frame(&mut tcx, options.width, 526);
    snafu::ensure!(
        source.last_error.is_none(),
        InputSnafu {
            message: "view declaration failed"
        }
    );
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, &mut tcx, (options.width, 526), 1.);
    let out = options.out.to_str().ok_or(Error::Input {
        message: "output must be UTF-8",
    })?;
    let base = plico_shell::window_frame::clip_window_frame(
        &mut scene,
        [options.width, 526],
        14.,
        source.base_color(),
    );
    plico_shell::renderer::render_to_png(&scene, options.width, 526, base, out)
        .context(RenderSnafu)?;
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
