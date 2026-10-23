//! Real dylib + live host-owned worker verification, using explicit step messages.
#[path = "../src/service.rs"]
#[allow(dead_code)]
mod service;
use plico_dev::{host::Descriptor, services::HostService};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_services_demo::TaskStatus;
use plico_vello::{Source, reload::ReloadView};
use snafu::{ResultExt, Snafu};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};
#[derive(usage_rs::Cli)]
#[usage(
    bin = "services-reload-check",
    about = "Verify host worker continuity across real UI module replacements"
)]
struct Options {
    #[usage(long)]
    module: PathBuf,
    #[usage(long)]
    reset_module: Option<PathBuf>,
}
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("verification IO failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("module failed: {source}"))]
    Module { source: plico_dev::host::Error },
    #[snafu(display("JSON failed: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("worker did not wake: {source}"))]
    Wait { source: mpsc::RecvTimeoutError },
    #[snafu(display("{message}"))]
    Verify { message: &'static str },
}
type Result<T> = std::result::Result<T, Error>;
fn publish(descriptor: &Path, module: &Path) -> Result<()> {
    std::fs::write(
        descriptor,
        serde_json::to_vec(&Descriptor {
            version: plico_dev::PROTOCOL_VERSION,
            library: module.into(),
        })
        .context(JsonSnafu)?,
    )
    .context(IoSnafu)
}
fn frame(
    source: &mut Source<ReloadView<service::TaskService>>,
    tcx: &mut TextContext,
) -> Result<()> {
    source.build_frame(tcx, 620, 520);
    snafu::ensure!(
        source.last_error.is_none() && source.view.last_error.is_none(),
        VerifySnafu {
            message: "view failed to settle"
        }
    );
    Ok(())
}
fn click(
    source: &mut Source<ReloadView<service::TaskService>>,
    tcx: &mut TextContext,
    label: &str,
) -> Result<()> {
    let tree = source.adapter.semantics(&source.engine, 0);
    let node = tree
        .node_by_role(SemanticRole::Button, label, None)
        .ok_or(Error::Verify {
            message: "button missing",
        })?;
    snafu::ensure!(
        source.handle_semantic_action(SemanticAction::Click { target: node.id }),
        VerifySnafu {
            message: "click was not consumed"
        }
    );
    frame(source, tcx)
}
fn wait_for(
    source: &mut Source<ReloadView<service::TaskService>>,
    tcx: &mut TextContext,
    wakes: &mpsc::Receiver<()>,
    done: impl Fn(&plico_services_demo::TaskSnapshot) -> bool,
) -> Result<()> {
    for _ in 0..128 {
        source.poll_async();
        frame(source, tcx)?;
        if done(&source.view.services().snapshot()) {
            return Ok(());
        }
        wakes
            .recv_timeout(Duration::from_secs(2))
            .context(WaitSnafu)?;
    }
    Err(Error::Verify {
        message: "worker did not reach the expected state",
    })
}
fn run() -> Result<()> {
    let options = Options::parse();
    let directory = tempfile::tempdir().context(IoSnafu)?;
    let descriptor = directory.path().join("active.json");
    let original = std::fs::canonicalize(options.module).context(IoSnafu)?;
    publish(&descriptor, &original)?;
    let (worker, steps) = service::TaskService::controlled();
    let mut source = Source::with_clock(
        ReloadView::with_services(descriptor.clone(), worker).context(ModuleSnafu)?,
        plico_core::VirtualClock::default(),
    );
    let (tx, wakes) = mpsc::channel();
    source.set_wake_callback(Arc::new(move || {
        let _ = tx.send(());
    }));
    let mut tcx = TextContext::new();
    frame(&mut source, &mut tcx)?;
    let instance = source.view.services().snapshot().instance;
    click(&mut source, &mut tcx, "Start task")?;
    click(&mut source, &mut tcx, "Increment view counter")?;
    for _ in 0..4 {
        frame(&mut source, &mut tcx)?;
    }
    snafu::ensure!(
        source.view.services().snapshot().started == 1,
        VerifySnafu {
            message: "settling repeated a service command"
        }
    );
    steps.send(()).map_err(|_| Error::Verify {
        message: "worker disconnected",
    })?;
    wait_for(&mut source, &mut tcx, &wakes, |s| s.progress == 2)?;
    let revision = directory
        .path()
        .join(format!("revision.{}", std::env::consts::DLL_EXTENSION));
    std::fs::copy(&original, &revision).context(IoSnafu)?;
    publish(&descriptor, &revision)?;
    snafu::ensure!(
        source.view.activate().context(ModuleSnafu)?,
        VerifySnafu {
            message: "revision did not activate"
        }
    );
    frame(&mut source, &mut tcx)?;
    snafu::ensure!(
        source.view.state()["counter"] == 1,
        VerifySnafu {
            message: "compatible reload lost UI state"
        }
    );
    let broken = directory.path().join("broken.so");
    std::fs::write(&broken, b"invalid module").context(IoSnafu)?;
    publish(&descriptor, &broken)?;
    snafu::ensure!(
        source.view.activate().is_err(),
        VerifySnafu {
            message: "broken candidate was accepted"
        }
    );
    // The old view and the same live worker still progress after a rejected reload.
    publish(&descriptor, &revision)?;
    steps.send(()).map_err(|_| Error::Verify {
        message: "worker disconnected",
    })?;
    wait_for(&mut source, &mut tcx, &wakes, |s| s.progress == 4)?;
    if let Some(reset) = options.reset_module {
        let reset = std::fs::canonicalize(reset).context(IoSnafu)?;
        publish(&descriptor, &reset)?;
        snafu::ensure!(
            source.view.activate().context(ModuleSnafu)?,
            VerifySnafu {
                message: "schema revision did not activate"
            }
        );
        frame(&mut source, &mut tcx)?;
        snafu::ensure!(
            source.view.snapshot().reset.is_some() && source.view.state()["counter"] == 0,
            VerifySnafu {
                message: "incompatible UI did not reset"
            }
        );
    }
    let state = source.view.services().snapshot();
    snafu::ensure!(
        state.instance == instance
            && state.started == 1
            && state.progress == 4
            && state.status == TaskStatus::Running,
        VerifySnafu {
            message: "reload/reset replaced or restarted the service"
        }
    );
    for _ in 2..50 {
        steps.send(()).map_err(|_| Error::Verify {
            message: "worker disconnected",
        })?;
    }
    wait_for(&mut source, &mut tcx, &wakes, |s| {
        s.status == TaskStatus::Completed
    })?;
    snafu::ensure!(
        source.view.services().snapshot().completed == 1,
        VerifySnafu {
            message: "completion was duplicated"
        }
    );
    click(&mut source, &mut tcx, "Start task")?;
    click(&mut source, &mut tcx, "Cancel task")?;
    wait_for(&mut source, &mut tcx, &wakes, |s| {
        s.status == TaskStatus::Cancelled
    })?;
    snafu::ensure!(
        source.view.state().get("instance").is_none(),
        VerifySnafu {
            message: "host state leaked into the UI checkpoint"
        }
    );
    println!(
        "PASS: live worker identity/progress survive compatible reload, rejected candidate and optional UI reset; commands are single-delivery; completion and cancellation work"
    );
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
