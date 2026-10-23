use plico_backend::{Source, reload::ReloadView};
use plico_renderer::{FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
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
            if let DevCommand::Click { role, label } | DevCommand::Focus { role, label } = &command
            {
                let role = SemanticRole::from_name(role).ok_or_else(|| Error::Input {
                    message: "unknown semantic role".into(),
                })?;
                let tree = source.adapter.semantics(&source.engine, 0);
                let n = tree
                    .node_by_role(role, label, None)
                    .ok_or_else(|| Error::Input {
                        message: format!("control not found: {label}"),
                    })?;
                source.handle_semantic_action(if matches!(command, DevCommand::Focus { .. }) {
                    SemanticAction::Focus { target: n.id }
                } else {
                    SemanticAction::Click { target: n.id }
                });
            }
            if let DevCommand::SetValue { label, value } = &command {
                let tree = source.adapter.semantics(&source.engine, 0);
                let node = tree
                    .node_by_role(SemanticRole::Slider, label, None)
                    .ok_or_else(|| Error::Input {
                        message: "slider not found".into(),
                    })?;
                source.handle_semantic_action(SemanticAction::SetNumericValue {
                    target: node.id,
                    value: *value,
                });
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
            match &command {
                DevCommand::Text { text } => {
                    source.prepare_for_event(&mut tcx);
                    source.handle_event(plico_renderer::UiEvent::TextInput(text.clone()));
                }
                DevCommand::Key { key } => {
                    source.prepare_for_event(&mut tcx);
                    source.handle_event(plico_renderer::UiEvent::Key(plico_renderer::KeyEvent {
                        phase: plico_renderer::KeyPhase::Down,
                        key: key.clone(),
                        key_without_modifiers: key.clone(),
                        code: String::new(),
                        text: None,
                        text_with_all_modifiers: None,
                        location: plico_renderer::KeyLocation::Standard,
                        modifiers: plico_renderer::Modifiers::empty(),
                        repeat: false,
                        synthetic: false,
                    }));
                }
                _ => {}
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
                serde_json::json!({"tooltip":tooltip,"state":source.view.state(),"nodes":source.view.snapshot().nodes,"reloads":source.view.reloads,"error":source.view.last_error,"pid":std::process::id()}),
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
        message:
            "this development host requires --module; use plico dev or cargo run -p plico-gallery"
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
