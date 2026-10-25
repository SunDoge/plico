use plico_renderer::{FrameSource, Scene, SemanticRole, TextContext};
use plico_vello::{Source, reload::ReloadView};
use snafu::{ResultExt, Snafu};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("filesystem or command IO failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("renderer failed: {source}"))]
    Render { source: plico_shell::Error },
    #[snafu(display("reload failed: {source}"))]
    Reload { source: plico_dev::host::Error },
    #[snafu(display("invalid developer command: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("{message}"))]
    Input { message: String },
}
type Result<T> = std::result::Result<T, Error>;
#[derive(usage_rs::Cli)]
#[usage(bin = "plico-host", about = "Plico rendering host", version)]
struct Cli {
    #[usage(subcommand)]
    command: Mode,
}
#[derive(usage_rs::Subcommands)]
enum Mode {
    Dev {
        #[usage(long)]
        module: Option<PathBuf>,
        #[usage(long, requires = "--module")]
        headless: bool,
        #[usage(flatten)]
        options: Options,
    },
    Render {
        #[usage(long)]
        out: PathBuf,
        #[usage(flatten)]
        options: Options,
    },
}
#[derive(usage_rs::Args, Default)]
struct Options {
    #[usage(long, default = "920")]
    width: u32,
    #[usage(long, default = "780")]
    height: u32,
    #[usage(long)]
    dark: bool,
    #[usage(long)]
    large: bool,
}
fn capture(source: &mut dyn FrameSource, options: &Options, out: &Path) -> Result<()> {
    if let Some(p) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(p).context(IoSnafu)?;
    }
    let mut tcx = TextContext::new();
    let mut scene = Scene::default();
    if source
        .render_retained_frame(&mut scene, &mut tcx, (options.width, options.height), 1.)
        .is_none()
    {
        let nodes = source.build_frame(&mut tcx, options.width, options.height);
        source.paint_scene(
            &mut scene,
            &nodes,
            &mut tcx,
            (options.width, options.height),
            1.,
        );
        source.paint_debug_overlay(&mut scene, &nodes, &mut tcx, 1.);
    }
    let path = out.to_str().ok_or_else(|| Error::Input {
        message: "PNG path must be UTF-8".into(),
    })?;
    plico_shell::renderer::render_to_png(
        &scene,
        options.width,
        options.height,
        source.base_color(),
        path,
    )
    .context(RenderSnafu)
}
#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum DevCommand {
    Snapshot,
    Scroll { x: f64, y: f64, delta: f64 },
    Click { role: String, label: String },
    Capture { out: PathBuf },
    SetValue { label: String, value: f64 },
    Focus { role: String, label: String },
    Text { text: String },
    Key { key: String },
}
fn headless(mut source: Source<ReloadView>, options: Options) -> Result<()> {
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, options.width, options.height);
    println!(
        "{}",
        serde_json::json!({"event":"ready","pid":std::process::id()})
    );
    std::io::stdout().flush().context(IoSnafu)?;
    for line in std::io::stdin().lock().lines() {
        let line = line.context(IoSnafu)?;
        let response = (|| -> Result<serde_json::Value> {
            match source.view.activate() {
                Ok(true) => source.request_rebuild(),
                Ok(false) => {}
                Err(error) => source.view.last_error = Some(error.to_string()),
            }
            let command: DevCommand = serde_json::from_str(&line).context(JsonSnafu)?;
            use plico_runtime::testing::Command;
            let shared = match &command {
                DevCommand::Snapshot => Some(Command::Snapshot),
                DevCommand::Click { role, label } => Some(Command::Click {
                    role: role.clone(),
                    label: label.clone(),
                    occurrence: None,
                }),
                DevCommand::Focus { role, label } => Some(Command::Focus {
                    role: role.clone(),
                    label: label.clone(),
                    occurrence: None,
                }),
                DevCommand::SetValue { label, value } => Some(Command::SetValue {
                    label: label.clone(),
                    value: *value,
                }),
                DevCommand::Text { text } => Some(Command::Text { text: text.clone() }),
                DevCommand::Key { key } => Some(Command::Key { key: key.clone() }),
                _ => None,
            };
            if let Some(shared) = shared {
                plico_runtime::testing::dispatch(&mut source, &mut tcx, &shared).map_err(
                    |error| Error::Input {
                        message: error.to_string(),
                    },
                )?;
            }
            if let DevCommand::Scroll { x, y, delta } = &command {
                source.handle_event(plico_renderer::UiEvent::Wheel(plico_renderer::WheelEvent {
                    position: plico_renderer::Point { x: *x, y: *y },
                    delta_x: 0.,
                    delta_y: *delta,
                    delta_mode: plico_renderer::WheelDeltaMode::Pixel,
                    phase: plico_renderer::GesturePhase::Changed,
                    modifiers: plico_renderer::Modifiers::empty(),
                }));
            }
            source.build_frame(&mut tcx, options.width, options.height);
            if let DevCommand::Capture { out } = command {
                capture(&mut source, &options, &out)?;
            }
            let tree = source.adapter.semantics(&source.engine, 0);
            let tooltip = tree
                .nodes
                .iter()
                .find(|n| {
                    n.role == SemanticRole::Tooltip
                        && n.bounds[2] > n.bounds[0]
                        && n.bounds[3] > n.bounds[1]
                })
                .map(|n| serde_json::json!({"text":n.label,"bounds":n.bounds,"focus":tree.focus}));
            Ok(
                serde_json::json!({"tooltip":tooltip,"state":source.view.state().decode::<serde_json::Value>().ok(),"state_bytes":source.view.state(),"nodes":source.view.snapshot().nodes,"reloads":source.view.reloads,"error":source.view.last_error,"pid":std::process::id()}),
            )
        })();
        match response {
            Ok(v) => println!("{v}"),
            Err(e) => println!("{}", serde_json::json!({"error":e.to_string()})),
        };
        std::io::stdout().flush().context(IoSnafu)?;
    }
    Ok(())
}
fn static_source(options: &Options) -> Result<Box<dyn FrameSource>> {
    let _ = options;
    Err(Error::Input {
        message: "this development host requires --module; use plico dev or cargo run -p gallery"
            .into(),
    })
}

fn run() -> Result<()> {
    let command = if std::env::args_os().len() == 1 {
        Mode::Dev {
            module: None,
            headless: false,
            options: Options {
                width: 920,
                height: 780,
                ..Default::default()
            },
        }
    } else {
        Cli::parse().command
    };
    let options = match &command {
        Mode::Render { options, .. } | Mode::Dev { options, .. } => options,
    };
    snafu::ensure!(
        (1..=8192).contains(&options.width) && (1..=8192).contains(&options.height),
        InputSnafu {
            message: "dimensions must be between 1 and 8192"
        }
    );
    match command {
        Mode::Render { out, options } => {
            capture(static_source(&options)?.as_mut(), &options, &out)?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Mode::Dev {
            module,
            headless: headless_mode,
            options,
        } => {
            let source: Box<dyn FrameSource> = if let Some(module) = module {
                let source = Source::new(ReloadView::new(module).context(ReloadSnafu)?);
                if headless_mode {
                    return headless(source, options);
                }
                Box::new(source)
            } else {
                static_source(&options)?
            };
            plico_shell::run_window_with_options(
                source,
                plico_renderer::WindowOptions::new()
                    .title("Plico gallery")
                    .initial_inner_size(options.width, options.height)
                    .min_inner_size(640, 740),
            )
            .context(RenderSnafu)
        }
    }
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("plico-host: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
