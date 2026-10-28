use super::*;
use cargo_metadata::Message;
use notify::{Event, RecursiveMode, Watcher};
use std::{
    io::Write,
    process::{Child, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(serde::Serialize)]
struct Artifact {
    package: String,
    target: String,
    fresh: bool,
}
#[derive(serde::Serialize)]
struct BuildReport {
    package: String,
    elapsed_ms: u128,
    artifacts: Vec<Artifact>,
}
struct Built {
    path: PathBuf,
    report: BuildReport,
}
fn build_flags(module: bool, host_binary: Option<&str>, host_features: &[&str]) -> Vec<OsString> {
    if module {
        let mut flags = vec!["--lib".into(), "--no-default-features".into()];
        if !host_features.is_empty() {
            flags.extend(["--features".into(), host_features.join(",").into()]);
        }
        flags
    } else {
        let mut flags = vec!["--bin".into(), host_binary.unwrap_or("plico-vello").into()];
        if host_binary.is_none() {
            flags.extend([
                "--no-default-features".into(),
                "--features".into(),
                "hot-reload".into(),
            ]);
        } else if !host_features.is_empty() {
            flags.extend(["--features".into(), host_features.join(",").into()]);
        }
        flags
    }
}
fn build(
    directory: &Path,
    manifest: &Path,
    package: &str,
    module: bool,
    target: &Path,
    host_binary: Option<&str>,
    host_features: &[&str],
) -> Result<Built> {
    let start = Instant::now();
    let mut command = Command::new(cargo());
    command
        .current_dir(directory)
        .args(["build", "--manifest-path"])
        .arg(manifest)
        .args(["-p", package, "--message-format=json-render-diagnostics"]);
    command.arg("--target-dir").arg(target).args([
        "--config",
        "profile.dev.opt-level=1",
        "--config",
        "profile.dev.incremental=true",
    ]);
    command.args(build_flags(module, host_binary, host_features));
    let output = command.stderr(Stdio::inherit()).output().context(IoSnafu)?;
    let mut artifacts = Vec::new();
    let mut path = None;
    for message in Message::parse_stream(output.stdout.as_slice()) {
        match message.context(IoSnafu)? {
            Message::CompilerArtifact(a) => {
                if module
                    && a.target.name == package.replace('-', "_")
                    && let Some(p) = a
                        .filenames
                        .iter()
                        .find(|p| p.extension() == Some(env::consts::DLL_EXTENSION))
                {
                    path = Some(p.clone().into_std_path_buf());
                }
                if !module
                    && a.target.name == host_binary.unwrap_or("plico-vello")
                    && let Some(p) = a.executable
                {
                    path = Some(p.into_std_path_buf());
                }
                artifacts.push(Artifact {
                    package: a.package_id.to_string(),
                    target: a.target.name,
                    fresh: a.fresh,
                });
            }
            Message::CompilerMessage(m) => {
                if let Some(rendered) = m.message.rendered {
                    eprint!("{rendered}");
                }
            }
            _ => {}
        }
    }
    snafu::ensure!(output.status.success(), BuildSnafu { package });
    let path = path.ok_or_else(|| Error::Artifact {
        package: package.into(),
        kind: if module { "cdylib" } else { "executable" }.into(),
    })?;
    Ok(Built {
        path,
        report: BuildReport {
            package: package.into(),
            elapsed_ms: start.elapsed().as_millis(),
            artifacts,
        },
    })
}
fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).context(IoSnafu)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).context(IoSnafu)?;
    serde_json::to_writer(&mut file, value).context(JsonSnafu)?;
    file.flush().context(IoSnafu)?;
    file.persist(path).map_err(|e| e.error).context(IoSnafu)?;
    Ok(())
}
fn publish(module: Built, session: &Path, report_path: &Path, revision: u64) -> Result<()> {
    let image = session.join(format!(
        "revision-{revision}.{}",
        env::consts::DLL_EXTENSION
    ));
    std::fs::copy(&module.path, &image).context(IoSnafu)?;
    atomic_json(report_path, &module.report)?;
    atomic_json(
        &session.join("active.json"),
        &serde_json::json!({"version":plico_dev::PROTOCOL_VERSION,"library":image}),
    )?;
    eprintln!(
        "[plico] application build {revision} in {} ms; published {}",
        module.report.elapsed_ms,
        image.display()
    );
    Ok(())
}
fn host_binary(metadata: &serde_json::Value) -> Result<Option<&str>> {
    metadata
        .get("plico")
        .and_then(|v| v.get("host"))
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::Workspace {
                    message: "package.metadata.plico.host must name a nonempty binary target"
                        .into(),
                })
        })
        .transpose()
}
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
                Some("target" | ".git" | "node_modules" | "dist")
            )
        }) && (p.extension().is_some_and(|e| e == "rs")
            || p.file_name().is_some_and(|n| n == "Cargo.toml"))
    })
}
struct Host(Child);
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn reloadable(package: &cargo_metadata::Package) -> bool {
    package
        .targets
        .iter()
        .any(|t| t.crate_types.contains(&cargo_metadata::CrateType::CDyLib))
}
#[derive(Clone, Copy)]
pub(super) enum Mode {
    Dev,
    Run,
}
impl Mode {
    fn accepts(self, package: &cargo_metadata::Package) -> bool {
        match self {
            Self::Dev => reloadable(package),
            Self::Run => package.targets.iter().any(|t| t.is_bin()),
        }
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
                    Mode::Dev => "library must declare cdylib",
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
        .args(["--package", selected.name.as_str()])
        .env_remove("PLICO_DEV_MODULE");
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

pub fn run(
    directory: &Path,
    manifest: &Path,
    package: Option<&str>,
    framework: &Path,
    no_watch: bool,
    headless: bool,
    arguments: Vec<OsString>,
) -> Result<u8> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(manifest)
        .cargo_path(cargo())
        .no_deps()
        .exec()
        .context(MetadataSnafu)?;
    let root = metadata.workspace_root.as_std_path();
    let directory = directory.canonicalize().context(IoSnafu)?;
    let directory = directory.as_path();
    let selected = select_package(&metadata, directory, package, Mode::Dev)?;
    let package = selected.name.as_str();
    eprintln!(
        "[plico] application {package} from {}",
        selected.manifest_path.parent().unwrap()
    );
    let custom_host = host_binary(&selected.metadata)?;
    if let Some(host) = custom_host {
        snafu::ensure!(
            selected
                .targets
                .iter()
                .any(|target| target.name == host && target.is_bin()),
            WorkspaceSnafu {
                message: format!("Plico host binary {host} was not found in package {package}")
            }
        );
        snafu::ensure!(
            !headless,
            WorkspaceSnafu {
                message: "--headless is currently provided by the generic host; custom hosts supply their own test runner"
            }
        );
    }
    let host_features: &[&str] =
        if custom_host.is_some() && selected.features.contains_key("hot-reload") {
            &["hot-reload"]
        } else {
            &[]
        };
    // New apps expose an ABI-only feature; keep older apps with direct exports working.
    let module_features: &[&str] = if selected.features.contains_key("abi") {
        &["abi"]
    } else if selected
        .dependencies
        .iter()
        .any(|dependency| dependency.name == "plico-dev")
    {
        &["plico-dev/abi"]
    } else {
        &[]
    };
    let target = if env::var_os("CARGO_TARGET_DIR").is_some() {
        metadata.target_directory.clone().into_std_path_buf()
    } else {
        framework.join("target")
    };
    let target = target.as_path();
    let report_path = target
        .join("plico-dev")
        .join(package)
        .join("build-report.json");
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).context(WatchSnafu)?;
    if !no_watch {
        watcher
            .watch(root, RecursiveMode::Recursive)
            .context(WatchSnafu)?;
    }
    let generic_manifest = framework.join("Cargo.toml");
    let host = build(
        directory,
        if custom_host.is_some() {
            manifest
        } else {
            &generic_manifest
        },
        if custom_host.is_some() {
            package
        } else {
            "plico-vello"
        },
        false,
        target,
        custom_host,
        host_features,
    )?;
    std::fs::create_dir_all(target).context(IoSnafu)?;
    let session = tempfile::Builder::new()
        .prefix("plico-dev-")
        .tempdir_in(target)
        .context(IoSnafu)?;
    let module = build(
        directory,
        manifest,
        package,
        true,
        target,
        None,
        module_features,
    )?;
    publish(module, session.path(), &report_path, 0)?;
    let mut command = Command::new(host.path);
    command.current_dir(directory);
    if custom_host.is_some() {
        command.env("PLICO_DEV_MODULE", session.path().join("active.json"));
        eprintln!(
            "[plico] custom host is resident; changes to host/service implementation require restarting plico dev"
        );
    } else {
        command
            .args(["dev", "--module"])
            .arg(session.path().join("active.json"));
        if headless {
            command.arg("--headless");
        }
    }
    command.args(arguments);
    let mut child = Host(command.spawn().context(IoSnafu)?);
    let mut revision = 0;
    eprintln!(
        "[plico] resident host PID {}; app module {package}; report {}",
        child.0.id(),
        report_path.display()
    );
    loop {
        if let Some(status) = child.0.try_wait().context(IoSnafu)? {
            return Ok(status
                .code()
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1));
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) if relevant(&event) => {
                // Coalesce editor writes; source changes during compilation remain queued.
                while let Ok(event) = rx.recv_timeout(Duration::from_millis(120)) {
                    if let Err(e) = event {
                        eprintln!("[plico] watcher: {e}");
                    }
                }
                match build(
                    directory,
                    manifest,
                    package,
                    true,
                    target,
                    None,
                    module_features,
                ) {
                    Ok(module) => {
                        revision += 1;
                        publish(module, session.path(), &report_path, revision)?;
                    }
                    Err(e) => eprintln!("[plico] {e}"),
                }
            }
            Ok(Err(e)) => eprintln!("[plico] watcher: {e}"),
            _ => {}
        }
    }
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
    let facade = serde_json::to_string(&framework.join("crates/plico")).context(JsonSnafu)?;
    let crate_name = name.replace('-', "_");
    let manifest = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[lib]
crate-type = ["rlib", "cdylib"]

[features]
abi = ["plico/abi", "dep:stabby"]
default = ["desktop"]
desktop = ["plico/desktop"]

[[bin]]
name = "{name}"
path = "src/main.rs"
required-features = ["desktop"]

[dependencies]
plico = {{ path = {facade}, default-features = false }}

# Required by the native ABI export macro; UI modules build without desktop.
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
stabby = {{ version = "=72.1.16", optional = true }}

[profile.dev]
opt-level = 1
incremental = true

[profile.dev.package."*"]
opt-level = 2
"#
    );
    std::fs::write(path.join("Cargo.toml"), manifest).context(IoSnafu)?;
    std::fs::write(path.join(".gitignore"), "/target\n").context(IoSnafu)?;
    std::fs::write(
        path.join("src/lib.rs"),
        r#"use plico::prelude::*;

#[derive(Default, plico::serde::Serialize, plico::serde::Deserialize)]
#[serde(default, crate = "plico::serde")]
pub struct Application {
    count: u32,
}

impl plico::App for Application {
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

#[cfg(all(feature = "abi", not(target_arch = "wasm32")))]
plico::export_app!(Application);
"#,
    )
    .context(IoSnafu)?;
    std::fs::write(
        path.join("src/main.rs"),
        format!(
            r#"fn main() -> Result<(), plico::DesktopError> {{
    plico::run({crate_name}::Application::default(), "{name}")
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
    fn module_and_host_features_are_separate() {
        let module = build_flags(true, None, &["abi"]);
        assert!(module.contains(&"--lib".into()));
        assert!(module.contains(&"--no-default-features".into()));
        assert!(module.contains(&"--features".into()));
        assert!(module.contains(&"abi".into()));
        assert!(!module.contains(&"hot-reload".into()));
        assert!(!module.contains(&"--config".into()));
        let host = build_flags(false, Some("app"), &["hot-reload"]);
        assert!(!host.contains(&"--no-default-features".into()));
        assert_eq!(
            &host[host.len() - 2..],
            [OsString::from("--features"), OsString::from("hot-reload")]
        );
        let generic = build_flags(false, None, &[]);
        assert!(generic.contains(&"--no-default-features".into()));
        assert!(generic.contains(&"hot-reload".into()));
    }
    #[test]
    fn custom_host_metadata_is_explicit_and_validated() {
        assert_eq!(host_binary(&serde_json::json!({})).unwrap(), None);
        assert_eq!(
            host_binary(&serde_json::json!({"plico":{"host":"my-app"}})).unwrap(),
            Some("my-app")
        );
        assert!(host_binary(&serde_json::json!({"plico":{"host":true}})).is_err());
        assert!(host_binary(&serde_json::json!({"plico":{"host":""}})).is_err());
    }
    #[test]
    fn source_filter_ignores_generated_files() {
        let event = |p: &str| notify::Event::new(notify::EventKind::Any).add_path(p.into());
        assert!(relevant(&event("/repo/crates/app/src/lib.rs")));
        assert!(relevant(&event("/repo/Cargo.toml")));
        assert!(!relevant(&event("/repo/target/build/generated.rs")));
        assert!(!relevant(&event("/repo/.git/index")));
        assert!(!relevant(
            &notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Read))
                .add_path("/repo/src/lib.rs".into())
        ));
    }
}
