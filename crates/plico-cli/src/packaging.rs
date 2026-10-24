//! Native release bundles. No development host or reload module is shipped.
use super::*;
use cargo_metadata::Message;
use std::{fs, io::Write, process::Stdio};

pub struct Options {
    pub package: Option<String>,
    pub bin: Option<String>,
    pub features: Option<String>,
    pub no_default_features: bool,
    pub output: Option<PathBuf>,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Workspace {
        message: message.into(),
    }
}
fn relative(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    snafu::ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        WorkspaceSnafu {
            message: "bundle resource paths must be relative, without . or .."
        }
    );
    Ok(path)
}
fn copy_resource(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source).context(IoSnafu)?;
    if metadata.is_dir() {
        fs::create_dir_all(destination).context(IoSnafu)?;
        for entry in fs::read_dir(source).context(IoSnafu)? {
            let entry = entry.context(IoSnafu)?;
            copy_resource(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        fs::create_dir_all(destination.parent().unwrap()).context(IoSnafu)?;
        fs::copy(source, destination).context(IoSnafu)?;
    } else {
        return Err(invalid(format!(
            "bundle resources cannot contain symlinks or special files: {}",
            source.display()
        )));
    }
    Ok(())
}
pub fn run(directory: &Path, manifest: &Path, options: Options) -> Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(manifest)
        .cargo_path(cargo())
        .no_deps()
        .exec()
        .context(MetadataSnafu)?;
    let directory = directory.canonicalize().context(IoSnafu)?;
    let package = development::select_package(
        &metadata,
        &directory,
        options.package.as_deref(),
        development::Mode::Run,
    )?;
    let bins: Vec<_> = package.targets.iter().filter(|t| t.is_bin()).collect();
    let name = options
        .bin
        .as_deref()
        .or(package.default_run.as_deref())
        .or_else(|| (bins.len() == 1).then(|| bins[0].name.as_str()))
        .ok_or_else(|| invalid("multiple binary targets; select one with --bin"))?;
    snafu::ensure!(
        bins.iter().any(|b| b.name == name),
        WorkspaceSnafu {
            message: format!("binary {name} not found")
        }
    );
    let config = &package.metadata["plico"]["bundle"];
    let resources: Vec<&str> = match config.get("resources") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| invalid("bundle.resources must be an array"))?
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| invalid("bundle.resources entries must be strings"))
            })
            .collect::<Result<_>>()?,
    };
    for resource in &resources {
        relative(resource)?;
    }
    let output = options
        .output
        .map(|p| directory.join(p))
        .unwrap_or_else(|| {
            metadata
                .target_directory
                .as_std_path()
                .join("bundle")
                .join(format!(
                    "{}-{}-{}",
                    package.name,
                    env::consts::OS,
                    env::consts::ARCH
                ))
        });
    snafu::ensure!(
        !output.exists(),
        WorkspaceSnafu {
            message: format!(
                "destination already exists: {}; choose --output or remove it explicitly",
                output.display()
            )
        }
    );
    let mut command = Command::new(cargo());
    command
        .current_dir(&directory)
        .args(["build", "--release", "--locked", "--manifest-path"])
        .arg(manifest)
        .args([
            "-p",
            package.name.as_str(),
            "--bin",
            name,
            "--message-format=json-render-diagnostics",
        ])
        .env_remove("PLICO_DEV_MODULE")
        .stderr(Stdio::inherit());
    if options.no_default_features {
        command.arg("--no-default-features");
    }
    if let Some(features) = options.features {
        command.args(["--features", &features]);
    }
    let result = command.output().context(IoSnafu)?;
    let mut executable = None;
    for message in Message::parse_stream(result.stdout.as_slice()) {
        match message.context(IoSnafu)? {
            Message::CompilerArtifact(a)
                if a.package_id == package.id && a.target.name == name && a.target.is_bin() =>
            {
                executable = a.executable;
            }
            Message::CompilerMessage(m) => {
                if let Some(rendered) = m.message.rendered {
                    eprint!("{rendered}");
                }
            }
            _ => {}
        }
    }
    snafu::ensure!(
        result.status.success(),
        BuildSnafu {
            package: package.name.as_str()
        }
    );
    let executable = executable.ok_or_else(|| Error::Artifact {
        package: package.name.to_string(),
        kind: "executable".into(),
    })?;
    let parent = output.parent().unwrap();
    fs::create_dir_all(parent).context(IoSnafu)?;
    let staging = tempfile::tempdir_in(parent).context(IoSnafu)?;
    let root = staging.path();
    fs::create_dir(root.join("bin")).context(IoSnafu)?;
    let filename = executable.file_name().unwrap();
    snafu::ensure!(
        filename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        WorkspaceSnafu {
            message: "bundle binary name must contain only ASCII letters, digits, -, _ or ."
        }
    );
    fs::copy(&executable, root.join("bin").join(filename)).context(IoSnafu)?;
    let assets = root.join("resources");
    fs::create_dir(&assets).context(IoSnafu)?;
    for resource in resources {
        let path = relative(resource)?;
        copy_resource(
            &package
                .manifest_path
                .parent()
                .unwrap()
                .as_std_path()
                .join(path),
            &assets.join(path),
        )?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Resolve resources independently of the user's working directory.
        let launcher = root.join("launch");
        fs::write(&launcher, format!("#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nexport PLICO_RESOURCE_DIR=\"$root/resources\"\ncd \"$root/resources\"\nexec \"$root/bin/{filename}\" \"$@\"\n")).context(IoSnafu)?;
        fs::set_permissions(launcher, fs::Permissions::from_mode(0o755)).context(IoSnafu)?;
    }
    #[cfg(target_os = "linux")]
    let desktop_launcher = parent
        .canonicalize()
        .context(IoSnafu)?
        .join(output.file_name().unwrap())
        .join("launch")
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    #[cfg(target_os = "linux")]
    fs::write(root.join(format!("{}.desktop", package.name)), format!("[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\"\nTerminal=false\nCategories=Utility;\n", package.name, desktop_launcher)).context(IoSnafu)?;
    let mut file = fs::File::create(root.join("bundle.json")).context(IoSnafu)?;
    serde_json::to_writer_pretty(&mut file, &serde_json::json!({"package":package.name,"version":package.version,"binary":format!("bin/{filename}"),"os":env::consts::OS,"arch":env::consts::ARCH})).context(JsonSnafu)?;
    file.write_all(b"\n").context(IoSnafu)?;
    fs::rename(root, &output).context(IoSnafu)?;
    eprintln!("[plico] bundle: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_paths_stay_inside_bundle() {
        for path in ["", "/etc/passwd", "../secret", "assets/../secret"] {
            assert!(relative(path).is_err(), "{path}");
        }
        assert!(relative("assets/icons/logo.png").is_ok());
    }
}
