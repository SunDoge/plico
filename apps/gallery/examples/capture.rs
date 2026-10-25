//! Gallery verification tool; the application itself does not parse arguments.
use gallery::{Gallery, Page};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_vello::Source;
use snafu::{ResultExt, Snafu};
use std::path::PathBuf;

#[derive(usage_rs::Cli)]
#[usage(
    bin = "gallery-capture",
    about = "Capture gallery pages for visual verification"
)]
struct Options {
    #[usage(long)]
    out: PathBuf,
    #[usage(long, default = "1120")]
    width: u32,
    #[usage(long, default = "820")]
    height: u32,
    #[usage(long)]
    dark: bool,
    #[usage(long)]
    large: bool,
    #[usage(long, default = "workspace")]
    page: String,
    #[usage(long)]
    game_sample: bool,
    #[usage(long)]
    markdown_complete: bool,
    #[usage(long, default = "1000")]
    game_ms: u64,
    #[usage(long)]
    game_move: bool,
    #[usage(long)]
    expanded: bool,
    #[usage(long)]
    compact: bool,
    #[usage(long)]
    empty: bool,
    #[usage(long)]
    dirty: bool,
    #[usage(long)]
    modal: bool,
    #[usage(long)]
    popover: bool,
    #[usage(long)]
    select: bool,
    #[usage(long)]
    menu: bool,
    #[usage(long)]
    tooltip: bool,
    /// Render the Glass page with backdrop blur disabled, for diffing.
    #[usage(long)]
    glass_off: bool,
}
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("capture IO failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("capture failed: {source}"))]
    Render { source: plico_shell::Error },
    #[snafu(display("{message}"))]
    Input { message: String },
}
fn run() -> Result<(), Error> {
    let options = Options::parse();
    snafu::ensure!(
        (1..=8192).contains(&options.width) && (1..=8192).contains(&options.height),
        InputSnafu {
            message: "dimensions must be between 1 and 8192"
        }
    );
    let page = match options.page.as_str() {
        "workspace" => Page::Workspace,
        "layout" => Page::Layout,
        "controls" => Page::Controls,
        "motion" => Page::Motion,
        "lists" => Page::Lists,
        "2048" => Page::Game2048,
        "markdown" => Page::Markdown,
        "glass" => Page::Glass,
        "calendar" => Page::Calendar,
        _ => {
            return Err(Error::Input {
                message:
                    "page must be workspace, layout, controls, motion, lists, 2048, markdown, glass or calendar"
                        .into(),
            });
        }
    };
    if let Some(parent) = options.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).context(IoSnafu)?;
    }
    let mut app = Gallery {
        dark: options.dark,
        large: options.large,
        expanded: options.expanded,
        release_details: options.expanded,
        compact: options.compact,
        router: plico_core::Router::new(page),
        ..Default::default()
    };
    if options.markdown_complete {
        app.markdown_chat.burst();
        app.markdown_chat.stream.reduced_motion = true;
        app.markdown_chat.follow = false;
    }
    if options.game_sample {
        app.game2048 = gallery::game2048::Game::from_cells([
            [2, 2, 4, 8],
            [4, 8, 16, 32],
            [8, 16, 64, 128],
            [0, 0, 256, 512],
        ]);
        app.game2048.score = 2948;
        app.game2048.best = 8192;
    }
    if options.game_move {
        app.game2048.move_dir(gallery::game2048::Direction::Left);
    }
    if options.empty {
        for project in &mut app.workspace.projects {
            project.archived = true;
        }
        app.workspace.selected = 0;
    }
    if options.dirty {
        app.workspace.projects[0].draft.autosave = true;
    }
    if options.modal {
        app.dialog.open();
    }
    if options.popover && page != Page::Calendar {
        app.quick_settings.open();
    }
    if options.select {
        app.channel_select.open();
    }
    if options.menu {
        app.quick_settings.open();
        app.action_menu.open();
    }
    if options.glass_off {
        app.glass.enabled = false;
    }
    let clock = plico_core::VirtualClock::default();
    let mut source = Source::with_clock(app, clock.clone());
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, options.width, options.height);
    if options.popover && page == Page::Calendar {
        let target = source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(plico_renderer::SemanticRole::Button, "Pick a date", None)
            .ok_or_else(|| Error::Input {
                message: "calendar picker trigger missing".into(),
            })?
            .id;
        source.handle_semantic_action(plico_renderer::SemanticAction::Click { target });
        source.build_frame(&mut tcx, options.width, options.height);
    }
    if options.tooltip {
        let tree = source.adapter.semantics(&source.engine, 0);
        let target = tree
            .node_by_role(plico_renderer::SemanticRole::Button, "Increment", None)
            .ok_or_else(|| Error::Input {
                message: "tooltip capture needs the Controls page without an open popup".into(),
            })?
            .id;
        source.handle_semantic_action(plico_renderer::SemanticAction::Focus { target });
    }
    clock
        .advance(std::time::Duration::from_millis(options.game_ms))
        .map_err(|_| Error::Input {
            message: "capture clock overflow".into(),
        })?;
    let nodes = source.build_frame(&mut tcx, options.width, options.height);
    let mut scene = Scene::new();
    source.paint_scene(
        &mut scene,
        &nodes,
        &mut tcx,
        (options.width, options.height),
        1.,
    );
    let path = options.out.to_str().ok_or_else(|| Error::Input {
        message: "output path must be UTF-8".into(),
    })?;
    plico_shell::renderer::render_to_png(
        &scene,
        options.width,
        options.height,
        source.base_color(),
        path,
    )
    .context(RenderSnafu)?;
    println!("wrote {}", options.out.display());
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
