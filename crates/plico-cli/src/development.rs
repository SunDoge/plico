use super::*;
use cargo_metadata::Message;
use notify::{Event, RecursiveMode, Watcher};
use std::{
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
fn relevant(event: &Event) -> bool {
    if matches!(
        event.kind,
        notify::EventKind::Access(_) | notify::EventKind::Other
    ) {
        return false;
    }
    event.paths.iter().any(|p| {
        !p.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some("target" | ".git" | "node_modules" | "dist" | "reports")
            )
        }) && (p.extension().is_some_and(|e| e == "rs")
            || p.extension()
                .is_some_and(|e| e == "json" || e == "png" || e == "svg")
            || p.file_name()
                .is_some_and(|n| n == "Cargo.toml" || n == "Cargo.lock"))
    })
}
#[derive(Clone, Copy)]
pub(super) enum Mode {
    Dev,
    Run,
}
impl Mode {
    fn accepts(self, package: &cargo_metadata::Package) -> bool {
        package.targets.iter().any(|t| t.is_bin())
    }
    fn command(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Run => "run",
        }
    }
}
pub(super) fn select_package<'m>(
    metadata: &'m cargo_metadata::Metadata,
    directory: &Path,
    explicit: Option<&str>,
    mode: Mode,
) -> Result<&'m cargo_metadata::Package> {
    let members: Vec<_> = metadata
        .packages
        .iter()
        .filter(|p| metadata.workspace_members.contains(&p.id))
        .collect();
    let selected = if let Some(name) = explicit {
        members
            .iter()
            .copied()
            .find(|p| p.name.as_str() == name)
            .ok_or_else(|| Error::Workspace {
                message: format!("workspace package {name} not found"),
            })?
    } else if let Some(package) = members
        .iter()
        .copied()
        .filter(|p| directory.starts_with(p.manifest_path.parent().unwrap().as_std_path()))
        .max_by_key(|p| p.manifest_path.as_str().len())
    {
        package
    } else {
        let defaults: Vec<_> = members
            .iter()
            .copied()
            .filter(|p| metadata.workspace_default_members.contains(&p.id) && mode.accepts(p))
            .collect();
        let candidates = if defaults.is_empty() {
            members
                .iter()
                .copied()
                .filter(|p| mode.accepts(p))
                .collect()
        } else {
            defaults
        };
        match candidates.as_slice() {
            [package] => *package,
            [] => {
                return Err(Error::Workspace {
                    message: format!(
                        "no application found for plico {}; choose an application directory",
                        mode.command()
                    ),
                });
            }
            _ => {
                return Err(Error::Workspace {
                    message: format!(
                        "multiple applications found; run plico {} <directory> (or use -p): {}",
                        mode.command(),
                        candidates
                            .iter()
                            .map(|p| {
                                p.manifest_path
                                    .parent()
                                    .unwrap()
                                    .as_std_path()
                                    .strip_prefix(metadata.workspace_root.as_std_path())
                                    .unwrap_or(p.manifest_path.parent().unwrap().as_std_path())
                                    .display()
                                    .to_string()
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
        }
    };
    snafu::ensure!(
        mode.accepts(selected),
        WorkspaceSnafu {
            message: format!(
                "package {} in {} cannot be used with plico {}: {}; choose an application directory",
                selected.name,
                directory.display(),
                mode.command(),
                match mode {
                    Mode::Dev => "no executable target",
                    Mode::Run => "no executable target",
                }
            )
        }
    );
    Ok(selected)
}

pub struct RunOptions {
    pub package: Option<String>,
    pub release: bool,
    pub bin: Option<String>,
    pub features: Option<String>,
    pub no_default_features: bool,
    pub arguments: Vec<OsString>,
}
pub fn run_static(directory: &Path, manifest: &Path, options: RunOptions) -> Result<u8> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(manifest)
        .cargo_path(cargo())
        .no_deps()
        .exec()
        .context(MetadataSnafu)?;
    let directory = directory.canonicalize().context(IoSnafu)?;
    let selected = select_package(&metadata, &directory, options.package.as_deref(), Mode::Run)?;
    let mut command = Command::new(cargo());
    command
        .current_dir(&directory)
        .args(["run", "--manifest-path"])
        .arg(manifest)
        .args(["--package", selected.name.as_str()]);
    if options.release {
        command.arg("--release");
    }
    if let Some(bin) = options.bin {
        command.args(["--bin", &bin]);
    }
    if options.no_default_features {
        command.arg("--no-default-features");
    }
    if let Some(features) = options.features {
        command.args(["--features", &features]);
    }
    command.arg("--").args(options.arguments);
    let status = command.status().context(IoSnafu)?;
    Ok(status
        .code()
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(1))
}

struct Host(Child);
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Build {
    executable: PathBuf,
    _directory: tempfile::TempDir,
}
fn build(directory: &Path, manifest: &Path, package: &str, binary: &str) -> Result<Build> {
    let start = Instant::now();
    let output = Command::new(cargo())
        .current_dir(directory)
        .args(["build", "--manifest-path"])
        .arg(manifest)
        .args([
            "--package",
            package,
            "--bin",
            binary,
            "--message-format=json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .context(IoSnafu)?;
    let mut executable = None;
    for message in Message::parse_stream(output.stdout.as_slice()) {
        match message.context(IoSnafu)? {
            Message::CompilerArtifact(a) if a.target.name == binary => {
                if let Some(path) = a.executable {
                    executable = Some(path.into_std_path_buf());
                }
            }
            Message::CompilerMessage(m) => {
                if let Some(text) = m.message.rendered {
                    eprint!("{text}");
                }
            }
            _ => {}
        }
    }
    snafu::ensure!(output.status.success(), BuildSnafu { package });
    let original = executable.ok_or_else(|| Error::Artifact {
        package: package.into(),
        kind: "executable".into(),
    })?;
    // Run a copy: Windows must be able to rebuild while the old executable is open.
    let staging = tempfile::tempdir().context(IoSnafu)?;
    let executable = staging.path().join(original.file_name().unwrap());
    std::fs::copy(&original, &executable).context(IoSnafu)?;
    eprintln!(
        "[plico] built {package} in {} ms",
        start.elapsed().as_millis()
    );
    Ok(Build {
        executable,
        _directory: staging,
    })
}
pub fn run(
    directory: &Path,
    manifest: &Path,
    package: Option<&str>,
    no_watch: bool,
    arguments: Vec<OsString>,
) -> Result<u8> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(manifest)
        .cargo_path(cargo())
        .no_deps()
        .exec()
        .context(MetadataSnafu)?;
    let directory = directory.canonicalize().context(IoSnafu)?;
    let selected = select_package(&metadata, &directory, package, Mode::Dev)?;
    let binaries: Vec<_> = selected.targets.iter().filter(|t| t.is_bin()).collect();
    let binary = selected
        .default_run
        .as_deref()
        .or_else(|| (binaries.len() == 1).then(|| binaries[0].name.as_str()))
        .ok_or_else(|| Error::Workspace {
            message: format!(
                "{} needs package.default-run when it has multiple binaries",
                selected.name
            ),
        })?;
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).context(WatchSnafu)?;
    if !no_watch {
        watcher
            .watch(
                metadata.workspace_root.as_std_path(),
                RecursiveMode::Recursive,
            )
            .context(WatchSnafu)?;
        for dependency in &selected.dependencies {
            if let Some(path) = &dependency.path
                && !path.starts_with(&metadata.workspace_root)
            {
                watcher
                    .watch(path.as_std_path(), RecursiveMode::Recursive)
                    .context(WatchSnafu)?;
            }
        }
    }
    if !no_watch
        && let Some(path) = arguments
            .windows(2)
            .find(|a| a[0] == "--scenario")
            .map(|a| Path::new(&a[1]))
        && !path.starts_with(metadata.workspace_root.as_std_path())
    {
        watcher
            .watch(path.parent().unwrap_or(path), RecursiveMode::NonRecursive)
            .context(WatchSnafu)?;
    }
    let scenario_path = arguments
        .windows(2)
        .find(|a| a[0] == "--scenario")
        .map(|a| PathBuf::from(&a[1]));
    let needs_build = |event: &Event| {
        event
            .paths
            .iter()
            .any(|path| scenario_path.as_ref() != Some(path))
    };
    let output_path = arguments
        .windows(2)
        .find(|a| a[0] == "--capture")
        .map(|a| PathBuf::from(&a[1]));
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    ctrlc::set_handler(move || {
        stop.store(true, Ordering::Release);
    })
    .map_err(|e| Error::Workspace {
        message: e.to_string(),
    })?;
    let launch = |built: &Build| -> Result<Host> {
        let child = Command::new(&built.executable)
            .current_dir(&directory)
            .args(&arguments)
            .spawn()
            .context(IoSnafu)?;
        eprintln!(
            "[plico] started PID {} (state resets on rebuild)",
            child.id()
        );
        Ok(Host(child))
    };
    let mut built = build(&directory, manifest, selected.name.as_str(), binary)?;
    let mut child = launch(&built)?;
    while !stopped.load(Ordering::Acquire) {
        if let Some(status) = child.0.try_wait().context(IoSnafu)?
            && (no_watch || !arguments.iter().any(|a| a == "--capture"))
        {
            return Ok(status
                .code()
                .and_then(|c| u8::try_from(c).ok())
                .unwrap_or(1));
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event))
                if !no_watch
                    && relevant(&event)
                    && event.paths.iter().any(|p| output_path.as_ref() != Some(p)) =>
            {
                let mut rebuild = needs_build(&event);
                let mut deadline = Instant::now() + Duration::from_millis(120);
                while let Ok(event) =
                    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                {
                    match event {
                        Ok(event)
                            if relevant(&event)
                                && event.paths.iter().any(|p| output_path.as_ref() != Some(p)) =>
                        {
                            rebuild |= needs_build(&event);
                            deadline = Instant::now() + Duration::from_millis(120);
                        }
                        Err(error) => eprintln!("[plico] watcher: {error}"),
                        _ => {}
                    }
                    if Instant::now() >= deadline {
                        break;
                    }
                }
                let next = if rebuild {
                    match build(&directory, manifest, selected.name.as_str(), binary) {
                        Ok(next) => Some(next),
                        Err(error) => {
                            eprintln!("[plico] {error}");
                            continue;
                        }
                    }
                } else {
                    eprintln!("[plico] scenario changed; reusing executable");
                    None
                };
                if child.0.try_wait().context(IoSnafu)?.is_none() {
                    child.0.kill().context(IoSnafu)?;
                }
                child.0.wait().context(IoSnafu)?;
                child = launch(next.as_ref().unwrap_or(&built))?;
                if let Some(next) = next {
                    built = next;
                }
            }
            Ok(Err(error)) => eprintln!("[plico] watcher: {error}"),
            _ => {}
        }
    }
    drop(child);
    drop(built);
    Ok(0)
}
pub fn create(path: &Path, name: Option<&str>, framework: &Path) -> Result<()> {
    snafu::ensure!(
        framework.join("crates/plico-dev/Cargo.toml").is_file(),
        WorkspaceSnafu {
            message: "framework sources not found; use --framework <Plico checkout>"
        }
    );
    let name = name
        .map(str::to_owned)
        .or_else(|| path.file_name().and_then(|n| n.to_str()).map(str::to_owned))
        .ok_or_else(|| Error::Workspace {
            message: "application needs a UTF-8 package name".into(),
        })?;
    snafu::ensure!(
        !name.is_empty()
            && name.starts_with(|c: char| c.is_ascii_alphabetic())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')),
        WorkspaceSnafu {
            message: "package name must start with a letter and contain letters, numbers, - or _"
        }
    );
    std::fs::create_dir(path).context(IoSnafu)?;
    std::fs::create_dir(path.join("src")).context(IoSnafu)?;
    let core = serde_json::to_string(&framework.join("crates/plico")).context(JsonSnafu)?;
    let dev = serde_json::to_string(&framework.join("crates/plico-dev")).context(JsonSnafu)?;
    let backend =
        serde_json::to_string(&framework.join("crates/plico-desktop")).context(JsonSnafu)?;
    let crate_name = name.replace('-', "_");
    let manifest = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[features]
default = ["desktop"]
desktop = ["dep:plico-desktop", "plico-desktop/wgpu"]

[[bin]]
name = "{name}"
path = "src/main.rs"
required-features = ["desktop"]

[dependencies]
plico = {{ path = {core} }}
plico-dev = {{ path = {dev} }}
plico-desktop = {{ path = {backend}, default-features = false, optional = true }}
serde = {{ version = "1", features = ["derive"] }}
usage-rs = "7"

[profile.dev]
opt-level = 0
incremental = true
debug = "line-tables-only"

[profile.dev.package."*"]
opt-level = 2
debug = false
"#
    );
    std::fs::write(path.join("Cargo.toml"), manifest).context(IoSnafu)?;
    std::fs::write(path.join(".gitignore"), "/target\n").context(IoSnafu)?;
    std::fs::write(
        path.join("src/lib.rs"),
        r#"use plico::prelude::*;

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Application {
    #[serde(default)]
    pub count: u32,
}

impl plico_dev::App for Application {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).gap(12.).padding(24.) + |cx| {
            text(cx, "Hello Plico").font_size(28);
            text!(cx, "Count: {}", self.count);
            if button(cx).text("Increment").clicked() {
                self.count += 1;
            }
        };
    }
}

"#,
    )
    .context(IoSnafu)?;
    std::fs::write(
        path.join("src/main.rs"),
        format!(
            r#"#[derive(usage_rs::Cli)]
struct Options {{
    #[usage(long)]
    scenario: Option<std::path::PathBuf>,
    #[usage(long)]
    capture: Option<std::path::PathBuf>,
}}
fn main() -> Result<(), Box<dyn std::error::Error>> {{
    let options = Options::parse();
    if let Some(output) = options.capture {{
        let scenario = if let Some(path) = options.scenario {{
            plico_desktop::PreviewScenario::<{crate_name}::Application>::load(path)?
        }} else {{ Default::default() }};
        scenario.render(|app| plico_desktop::NativeApp(app))?.write_png(output)?;
        return Ok(());
    }}
    plico_desktop::run({crate_name}::Application::default(), "{name}")?;
    Ok(())
}}
"#
        ),
    )
    .context(IoSnafu)?;
    eprintln!(
        "[plico] created {}. Run: plico -C '{}' dev",
        path.display(),
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starter_has_static_app_and_preview_entry() {
        let temp = tempfile::tempdir().unwrap();
        let framework = temp.path().join("framework");
        std::fs::create_dir_all(framework.join("crates/plico-dev")).unwrap();
        std::fs::write(framework.join("crates/plico-dev/Cargo.toml"), "").unwrap();
        let app = temp.path().join("app");
        create(&app, None, &framework).unwrap();
        let manifest = std::fs::read_to_string(app.join("Cargo.toml")).unwrap();
        let main = std::fs::read_to_string(app.join("src/main.rs")).unwrap();
        assert!(!manifest.contains("stabby"));
        assert!(!manifest.contains("cdylib"));
        assert!(main.contains("PreviewScenario"));
    }
    #[test]
    fn source_filter_ignores_generated_files() {
        let event = |p: &str| notify::Event::new(notify::EventKind::Any).add_path(p.into());
        assert!(relevant(&event("/repo/src/lib.rs")));
        assert!(!relevant(&event("/repo/target/generated.rs")));
    }
}
