use super::*;
use cargo_metadata::Message;
use notify::{Event, RecursiveMode, Watcher};
use std::{
    cell::OnceCell,
    collections::HashSet,
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
fn ignored_dir(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some("target" | ".git" | "node_modules" | "dist" | "reports")
    )
}

fn source_file(path: &Path) -> bool {
    !path.components().any(|c| ignored_dir(c.as_os_str()))
        && (matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "json" | "png" | "svg" | "wgsl" | "ftl")
        ) || path
            .file_name()
            .is_some_and(|n| n == "Cargo.toml" || n == "Cargo.lock"))
}

#[cfg(target_os = "linux")]
// Subscribe before enumerating children so a directory created during the walk
// also arrives as an event. Never descend into Cargo output or symlink cycles.
fn watch_tree(watcher: &mut notify::RecommendedWatcher, root: &Path, target: &Path) -> Result<()> {
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        if dir == target || dir.join(".rustc_info.json").is_file() {
            continue;
        }
        match watcher.watch(&dir, RecursiveMode::NonRecursive) {
            Err(_) if !dir.exists() => continue,
            result => result.context(WatchSnafu)?,
        }
        let entries = match std::fs::read_dir(&dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            result => result.context(IoSnafu)?,
        };
        for entry in entries {
            let entry = entry.context(IoSnafu)?;
            if ignored_dir(&entry.file_name()) {
                continue;
            }
            let kind = match entry.file_type() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                result => result.context(IoSnafu)?,
            };
            if kind.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn watch_tree(watcher: &mut notify::RecommendedWatcher, root: &Path, _: &Path) -> Result<()> {
    watcher
        .watch(root, RecursiveMode::Recursive)
        .context(WatchSnafu)
}

// Notify reports absolute paths, while application arguments are relative to
// the launched process. Resolve the parent even when the output does not exist.
fn argument_path(arguments: &[OsString], flag: &str, directory: &Path) -> Option<PathBuf> {
    let path = directory.join(&arguments.windows(2).find(|a| a[0] == flag)?[1]);
    Some(
        match (
            path.parent().and_then(|p| p.canonicalize().ok()),
            path.file_name(),
        ) {
            (Some(parent), Some(name)) => parent.join(name),
            _ => path,
        },
    )
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
// Cargo also records include_bytes!/include_str! inputs owned by other
// packages. If dep-info is unavailable, keep the conservative rebuild.
fn cargo_inputs(dep_info: &Path) -> Option<HashSet<PathBuf>> {
    let contents = std::fs::read_to_string(dep_info).ok()?;
    let (_, dependencies) = contents.trim_end().split_once(": ")?;
    let mut paths = HashSet::new();
    let mut word = String::new();
    for part in dependencies.split_inclusive(' ') {
        if let Some(prefix) = part.strip_suffix("\\ ") {
            word.push_str(prefix);
            word.push(' ');
            continue;
        }
        word.push_str(part.trim_end_matches(' '));
        let path = Path::new(&word);
        if !path.is_absolute() {
            return None;
        }
        // Keep normalized paths even when an editor temporarily removes a file.
        let mut normalized = PathBuf::new();
        for component in path.components() {
            if component == std::path::Component::ParentDir {
                normalized.pop();
            } else {
                normalized.push(component.as_os_str());
            }
        }
        paths.insert(normalized);
        word.clear();
    }
    Some(paths)
}

struct Build {
    packages: HashSet<cargo_metadata::PackageId>,
    dep_info: PathBuf,
    inputs: OnceCell<Option<HashSet<PathBuf>>>,
    executable: PathBuf,
    _directory: tempfile::TempDir,
}
fn build(
    directory: &Path,
    manifest: &Path,
    package: &str,
    binary: &str,
    flags: &[String],
) -> Result<Build> {
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
        .args(flags)
        .stderr(Stdio::inherit())
        .output()
        .context(IoSnafu)?;
    let mut executable = None;
    let mut packages = HashSet::new();
    for message in Message::parse_stream(output.stdout.as_slice()) {
        match message.context(IoSnafu)? {
            Message::CompilerArtifact(a) => {
                packages.insert(a.package_id);
                if a.target.name == binary
                    && let Some(path) = a.executable
                {
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
        packages,
        dep_info: original.with_extension("d"),
        inputs: OnceCell::new(),
        executable,
        _directory: staging,
    })
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct WasmDev {
    host: String,
    guest: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    host_features: Vec<String>,
    watch: Vec<PathBuf>,
}
impl WasmDev {
    fn build_flags(&self, guest: bool) -> Vec<String> {
        let mut flags = Vec::new();
        let mut features = self.features.clone();
        if guest {
            flags.extend([
                "--target".into(),
                "wasm32-unknown-unknown".into(),
                "--no-default-features".into(),
            ]);
        } else {
            features.extend_from_slice(&self.host_features);
        }
        if !features.is_empty() {
            flags.extend(["--features".into(), features.join(",")]);
        }
        flags
    }

    fn publish(
        &self,
        directory: &Path,
        manifest: &Path,
        package: &str,
        module: &Path,
    ) -> Result<()> {
        let built = build(
            directory,
            manifest,
            package,
            &self.guest,
            &self.build_flags(true),
        )?;
        let pending = tempfile::NamedTempFile::new_in(module.parent().unwrap()).context(IoSnafu)?;
        std::fs::copy(built.executable, pending.path()).context(IoSnafu)?;
        pending
            .persist(module)
            .map_err(|e| Error::Io { source: e.error })?;
        eprintln!("[plico] published {}", self.guest);
        Ok(())
    }
}
pub fn run(
    directory: &Path,
    manifest: &Path,
    package: Option<&str>,
    no_watch: bool,
    wasm: bool,
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
    let wasm: Option<WasmDev> = if wasm {
        let config = selected
            .metadata
            .pointer("/plico/wasm")
            .ok_or_else(|| Error::Workspace {
                message: format!(
                    "{} needs [package.metadata.plico.wasm] for dev --wasm",
                    selected.name
                ),
            })?;
        Some(serde_json::from_value(config.clone()).context(JsonSnafu)?)
    } else {
        None
    };
    let root = selected.manifest_path.parent().unwrap().as_std_path();
    let watch_paths: Vec<_> = wasm
        .iter()
        .flat_map(|w| &w.watch)
        .map(|p| root.join(p))
        .collect();
    if wasm.is_some()
        && !no_watch
        && (watch_paths.is_empty() || watch_paths.iter().any(|p| !p.exists()))
    {
        return Err(Error::Workspace {
            message: "wasm.watch must name existing guest source files or directories".into(),
        });
    }
    let binaries: Vec<_> = selected.targets.iter().filter(|t| t.is_bin()).collect();
    let binary = wasm
        .as_ref()
        .map(|w| w.host.as_str())
        .or(selected.default_run.as_deref())
        .or_else(|| (binaries.len() == 1).then(|| binaries[0].name.as_str()))
        .ok_or_else(|| Error::Workspace {
            message: format!(
                "{} needs package.default-run when it has multiple binaries",
                selected.name
            ),
        })?;
    let (tx, rx) = mpsc::channel();
    let scenario_path = argument_path(&arguments, "--scenario", &directory);
    let output_path = argument_path(&arguments, "--capture", &directory);
    let mut source_roots = vec![metadata.workspace_root.as_std_path()];
    source_roots.extend(
        selected
            .dependencies
            .iter()
            .filter_map(|d| d.path.as_ref())
            .filter(|p| !p.starts_with(&metadata.workspace_root))
            .map(|p| p.as_std_path()),
    );
    let workspace = metadata.workspace_root.as_std_path();
    let target = metadata.target_directory.as_std_path();
    let mut members: Vec<_> = metadata
        .packages
        .iter()
        .filter_map(|pkg| {
            let dir = pkg.manifest_path.parent()?.as_std_path();
            (dir != workspace).then_some((dir, &pkg.id))
        })
        .collect();
    members.sort_unstable_by_key(|(dir, _)| std::cmp::Reverse(dir.as_os_str().len()));
    let mut watcher = if no_watch {
        None
    } else {
        Some(notify::recommended_watcher(tx.clone()).context(WatchSnafu)?)
    };
    if let Some(watcher) = &mut watcher {
        if wasm.is_some() {
            for path in &watch_paths {
                let (dir, mode) = if path.is_dir() {
                    (path.as_path(), RecursiveMode::Recursive)
                } else {
                    (path.parent().unwrap(), RecursiveMode::NonRecursive)
                };
                watcher.watch(dir, mode).context(WatchSnafu)?;
            }
        } else {
            for root in &source_roots {
                watch_tree(watcher, root, target)?;
            }
        }
        if let Some(path) = &scenario_path
            && (cfg!(target_os = "linux") && wasm.is_none()
                || !source_roots.iter().any(|root| path.starts_with(root))
                || wasm.is_some() && !watch_paths.iter().any(|w| path.starts_with(w)))
        {
            watcher
                .watch(path.parent().unwrap_or(path), RecursiveMode::NonRecursive)
                .context(WatchSnafu)?;
        }
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    ctrlc::set_handler(move || {
        stop.store(true, Ordering::Release);
    })
    .map_err(|e| Error::Workspace {
        message: e.to_string(),
    })?;
    let module_dir = wasm
        .as_ref()
        .map(|_| tempfile::tempdir())
        .transpose()
        .context(IoSnafu)?;
    let module = module_dir.as_ref().map(|dir| dir.path().join("ui.wasm"));
    let mut change = |event: &Event, build: Option<&Build>| -> Result<Option<bool>> {
        if matches!(
            event.kind,
            notify::EventKind::Access(_) | notify::EventKind::Other
        ) {
            return Ok(None);
        }
        // Some(false) restarts the last successful build for a scenario edit.
        // Consider both sides of an atomic rename independently.
        let mut change = None;
        for path in &event.paths {
            if output_path.as_ref() == Some(path) {
                continue;
            }
            if scenario_path.as_ref() == Some(path) {
                change = change.max(Some(false));
                continue;
            }
            if wasm.is_some() {
                if watch_paths.iter().any(|w| path.starts_with(w)) {
                    change = Some(true);
                }
                continue;
            }
            let Some(relative) = source_roots
                .iter()
                .find_map(|root| path.strip_prefix(root).ok())
            else {
                continue;
            };
            if path.starts_with(target) || relative.components().any(|c| ignored_dir(c.as_os_str()))
            {
                continue;
            }
            let new_dir = matches!(
                event.kind,
                notify::EventKind::Create(_)
                    | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
            ) && path.symlink_metadata().is_ok_and(|m| m.is_dir());
            if cfg!(target_os = "linux")
                && new_dir
                && let Some(watcher) = &mut watcher
            {
                watch_tree(watcher, path, target)?;
            }
            if !(new_dir || source_file(relative)) {
                continue;
            }
            if let Some(build) = build
                && let Some((_, owner)) = members.iter().find(|(dir, _)| path.starts_with(dir))
                && !build.packages.contains(owner)
                && let Some(inputs) = build.inputs.get_or_init(|| cargo_inputs(&build.dep_info))
                && !inputs.contains(path)
            {
                continue;
            }
            change = Some(true);
        }
        Ok(change)
    };
    let launch = |built: &Build| -> Result<Host> {
        let mut command = Command::new(&built.executable);
        if let Some(module) = &module {
            command.env("PLICO_WASM_MODULE", module);
        }
        let child = command
            .current_dir(&directory)
            .args(&arguments)
            .spawn()
            .context(IoSnafu)?;
        eprintln!("[plico] started PID {}", child.id());
        Ok(Host(child))
    };
    let host_flags = wasm
        .as_ref()
        .map(|w| w.build_flags(false))
        .unwrap_or_default();
    let mut built = build(
        &directory,
        manifest,
        selected.name.as_str(),
        binary,
        &host_flags,
    )?;
    if let (Some(wasm), Some(module)) = (&wasm, &module) {
        wasm.publish(&directory, manifest, selected.name.as_str(), module)?;
    }
    let mut child = launch(&built)?;
    let mut build_failed = false;
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
            Ok(Ok(event)) => {
                let active = (!build_failed).then_some(&built);
                let Some(mut rebuild) = change(&event, active)? else {
                    continue;
                };
                let mut deadline = Instant::now() + Duration::from_millis(120);
                while let Ok(event) =
                    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                {
                    match event {
                        Ok(event) => {
                            if let Some(build) = change(&event, active)? {
                                rebuild |= build;
                                deadline = Instant::now() + Duration::from_millis(120);
                            }
                        }
                        Err(error) => eprintln!("[plico] watcher: {error}"),
                    }
                    if Instant::now() >= deadline {
                        break;
                    }
                }
                if rebuild && let (Some(wasm), Some(module)) = (&wasm, &module) {
                    if let Err(error) =
                        wasm.publish(&directory, manifest, selected.name.as_str(), module)
                    {
                        eprintln!("[plico] {error}");
                    }
                    continue;
                }
                let next = if rebuild {
                    match build(
                        &directory,
                        manifest,
                        selected.name.as_str(),
                        binary,
                        &host_flags,
                    ) {
                        Ok(next) => Some(next),
                        Err(error) => {
                            build_failed = true;
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
                    build_failed = false;
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
    #[usage(long)]
    record: Option<std::path::PathBuf>,
    #[usage(long, default = "60")]
    fps: u32,
}}
fn main() -> Result<(), Box<dyn std::error::Error>> {{
    let options = Options::parse();
    if options.capture.is_some() && options.record.is_some() {{
        return Err("--capture and --record are mutually exclusive".into());
    }}
    if let Some(output) = options.record {{
        let scenario = if let Some(path) = options.scenario {{
            plico_desktop::PreviewScenario::<{crate_name}::Application>::load(path)?
        }} else {{ Default::default() }};
        scenario.record_mp4(output, options.fps, plico_desktop::NativeApp)?;
        return Ok(());
    }}
    if let Some(output) = options.capture {{
        let scenario = if let Some(path) = options.scenario {{
            plico_desktop::PreviewScenario::<{crate_name}::Application>::load(path)?
        }} else {{ Default::default() }};
        scenario.render(|app| plico_desktop::NativeApp(app))?.write_png(output)?;
        return Ok(());
    }}
    plico_desktop::run({crate_name}::Application::default(), plico_desktop::WindowOptions::new().title("{name}"))?;
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
        for input in ["src/lib.rs", "shaders/blur.wgsl", "locales/zh.ftl"] {
            assert!(source_file(Path::new(input)));
        }
        assert!(!source_file(Path::new("target/generated.rs")));
    }
}
