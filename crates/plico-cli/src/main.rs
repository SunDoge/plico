use snafu::{ResultExt, Snafu};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
mod development;
mod formatting;
mod icons;
mod packaging;
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("icons: {message}"))]
    Icons { message: String },
    #[snafu(display("command IO failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("cannot locate Plico workspace: {message}"))]
    Workspace { message: String },
    #[snafu(display("invalid Cargo output: {source}"))]
    Utf8 { source: std::string::FromUtf8Error },
    #[snafu(display("Cargo metadata failed: {source}"))]
    Metadata { source: cargo_metadata::Error },
    #[snafu(display("file watcher failed: {source}"))]
    Watch { source: notify::Error },
    #[snafu(display("JSON failed: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("cannot format {}: {message}", path.display()))]
    Format { path: PathBuf, message: String },
    #[snafu(display("build failed for {package}; last good application remains active"))]
    Build { package: String },
    #[snafu(display("Cargo did not produce a {kind} artifact for {package}"))]
    Artifact { package: String, kind: String },
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(usage_rs::Cli)]
#[usage(
    bin = "plico",
    version,
    about = "Plico development CLI",
    arg_required_else_help = true
)]
struct Cli {
    #[usage(short = 'C', long = "directory", global = true)]
    directory: Option<PathBuf>,
    #[usage(long, global = true)]
    framework: Option<PathBuf>,
    #[usage(subcommand)]
    command: Action,
}
#[derive(usage_rs::Subcommands)]
enum Action {
    /// Generate application-owned SVG constants; normal builds need no generator.
    Icons {
        #[usage(subcommand)]
        command: icons::Action,
    },
    /// Create an immediate-mode Plico application outside the framework workspace.
    New {
        path: PathBuf,
        #[usage(long)]
        name: Option<String>,
    },
    /// Build and hot-reload an application module in a resident rendering host.
    Dev {
        /// Application directory; defaults to the current working directory.
        path: Option<PathBuf>,
        /// Advanced workspace package override; normally inferred from the directory.
        #[usage(short = 'p', long)]
        package: Option<String>,
        /// Build once, without watching source files.
        #[usage(long)]
        no_watch: bool,
        /// Run the resident host with JSON commands on stdin instead of opening a window.
        #[usage(long)]
        headless: bool,
        #[usage(double_dash = "required")]
        arguments: Vec<OsString>,
    },
    /// Run the statically linked application with its normal default features.
    Run {
        path: Option<PathBuf>,
        #[usage(short = 'p', long)]
        package: Option<String>,
        #[usage(long)]
        release: bool,
        #[usage(long)]
        bin: Option<String>,
        #[usage(long)]
        features: Option<String>,
        #[usage(long)]
        no_default_features: bool,
        #[usage(double_dash = "required")]
        arguments: Vec<OsString>,
    },
    /// Build a release application directory with resources and a Linux desktop entry.
    Bundle {
        path: Option<PathBuf>,
        #[usage(short = 'p', long)]
        package: Option<String>,
        #[usage(long)]
        bin: Option<String>,
        #[usage(long)]
        features: Option<String>,
        #[usage(long)]
        no_default_features: bool,
        /// Destination directory; must not already exist.
        #[usage(long)]
        output: Option<PathBuf>,
    },
    /// Render the static gallery to a PNG without opening a window.
    Render {
        #[usage(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    /// Print backend counters as JSON.
    Probe,
    Check {
        #[usage(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    Test {
        #[usage(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    /// Format ordinary Rust and skipped Plico components together.
    Fmt {
        /// Report unformatted files without writing them.
        #[usage(long)]
        check: bool,
        /// Select a workspace package (defaults to all workspace members).
        #[usage(short = 'p', long)]
        package: Option<String>,
        /// Format all workspace members (the default).
        #[usage(long)]
        all: bool,
        /// Maximum flattened component chain width (default: 120).
        #[usage(long)]
        width: Option<usize>,
        /// Optional Rust files or directories, relative to the working directory.
        paths: Vec<PathBuf>,
    },
    Clippy {
        #[usage(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
}
fn cargo() -> OsString {
    // Cargo injects CARGO=the-real-toolchain-binary into `cargo run`/subcommands.
    // Reusing that path bypasses user PATH wrappers such as mbx and mise.
    env::var_os("PLICO_CARGO").unwrap_or_else(|| "cargo".into())
}
fn workspace(directory: &Path) -> Result<PathBuf> {
    let output = Command::new(cargo())
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .current_dir(directory)
        .output()
        .context(IoSnafu)?;
    snafu::ensure!(
        output.status.success(),
        WorkspaceSnafu {
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned()
        }
    );
    let manifest = PathBuf::from(String::from_utf8(output.stdout).context(Utf8Snafu)?.trim());
    Ok(manifest)
}
fn cargo_arguments(name: &str, args: Vec<OsString>, manifest: &Path) -> Vec<OsString> {
    let prefix: &[&str] = match name {
        "render" => &["run", "--quiet", "-p", "gallery", "--example", "capture"],
        "probe" => &["run", "--quiet", "-p", "gallery", "--example", "probe"],
        "check" => &["check", "--workspace", "--all-targets"],
        "test" => &["test", "--workspace"],
        "clippy" => &["clippy", "--workspace", "--all-targets"],
        _ => unreachable!(),
    };
    let mut a: Vec<OsString> = prefix.iter().map(OsString::from).collect();
    a.extend(["--manifest-path".into(), manifest.as_os_str().into()]);
    if name == "render" {
        a.push("--".into());
    }
    if name == "probe" {
        a.push("--".into());
    }
    a.extend(args);
    a
}
fn run() -> Result<u8> {
    let raw: Vec<OsString> = env::args_os().collect();
    let tail = raw
        .iter()
        .position(|a| a == "--")
        .map(|i| raw.len() - i - 1);
    let cli = Cli::parse();
    let cwd = env::current_dir().context(IoSnafu)?;
    let base_directory = cli.directory.map_or(cwd.clone(), |d| cwd.join(d));
    let directory = match &cli.command {
        Action::Dev {
            path: Some(path), ..
        }
        | Action::Run {
            path: Some(path), ..
        }
        | Action::Bundle {
            path: Some(path), ..
        } => base_directory.join(path),
        _ => base_directory.clone(),
    };
    let framework = cli.framework.map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_owned()
        },
        |p| base_directory.join(p),
    );
    if let Action::New { path, name } = &cli.command {
        return development::create(&directory.join(path), name.as_deref(), &framework).map(|()| 0);
    }
    if let Action::Icons { command } = &cli.command {
        return icons::run(&directory, command).map(|()| 0);
    }
    let manifest = workspace(&directory)?;
    let (name, mut arguments) = match cli.command {
        Action::Dev {
            path: _,
            package,
            no_watch,
            headless,
            arguments,
        } => {
            return development::run(
                &directory,
                &manifest,
                package.as_deref(),
                &framework,
                no_watch,
                headless,
                arguments,
            );
        }
        Action::Run {
            path: _,
            package,
            release,
            bin,
            features,
            no_default_features,
            arguments,
        } => {
            return development::run_static(
                &directory,
                &manifest,
                development::RunOptions {
                    package,
                    release,
                    bin,
                    features,
                    no_default_features,
                    arguments,
                },
            );
        }
        Action::Bundle {
            package,
            bin,
            features,
            no_default_features,
            output,
            ..
        } => {
            return packaging::run(
                &directory,
                &manifest,
                packaging::Options {
                    package,
                    bin,
                    features,
                    no_default_features,
                    output,
                },
            )
            .map(|()| 0);
        }
        Action::New { .. } => unreachable!(),
        Action::Icons { .. } => unreachable!(),
        Action::Render { arguments } => ("render", arguments),
        Action::Probe => ("probe", Vec::new()),
        Action::Check { arguments } => ("check", arguments),
        Action::Test { arguments } => ("test", arguments),
        Action::Fmt {
            check,
            package,
            all,
            width,
            paths,
        } => {
            return formatting::run(
                &directory,
                &manifest,
                formatting::Options {
                    check,
                    package,
                    all,
                    width: width.unwrap_or(120),
                    paths,
                },
            );
        }
        Action::Clippy { arguments } => ("clippy", arguments),
    };
    if !matches!(name, "render" | "probe")
        && let Some(tail) = tail
    {
        let at = arguments.len().saturating_sub(tail);
        arguments.insert(at, "--".into());
    }
    let status = Command::new(cargo())
        .args(cargo_arguments(name, arguments, &manifest))
        .current_dir(directory)
        .status()
        .context(IoSnafu)?;
    Ok(status
        .code()
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(1))
}
fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("plico: {error}");
            ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse<'a>(a: &'a [&'a str]) -> std::result::Result<Cli, usage_rs::Error<'static, 'a>> {
        let args = a.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>();
        Cli::try_parse_from(&args)
    }
    #[test]
    fn usage_checks_commands_and_required_values() {
        for a in [
            vec!["plico", "typo"],
            vec!["plico", "-C"],
            vec!["plico", "dev", "--package"],
        ] {
            assert!(parse(&a).is_err());
        }
    }
    #[test]
    fn dev_defaults_to_hot_reload() {
        let c = parse(&["plico", "dev"]).unwrap();
        assert!(matches!(
            c.command,
            Action::Dev {
                package: None,
                no_watch: false,
                ..
            }
        ));
    }
    #[test]
    fn dev_takes_an_application_directory_and_separates_host_arguments() {
        let c = parse(&[
            "plico",
            "dev",
            "apps/my app",
            "--no-watch",
            "--",
            "--width",
            "920",
        ])
        .unwrap();
        assert!(
            matches!(c.command, Action::Dev { path: Some(path), package: None, no_watch: true, arguments, .. }
            if path == Path::new("apps/my app") && arguments == vec![OsString::from("--width"), OsString::from("920")])
        );
        assert!(parse(&["plico", "dev", "one", "two"]).is_err());
    }
    #[test]
    fn preserves_paths_and_arguments() {
        let c = parse(&["plico", "-C", "a b", "test", "--all-features"]).unwrap();
        assert_eq!(c.directory, Some("a b".into()));
        assert!(
            matches!(c.command,Action::Test{arguments}if arguments==vec![OsString::from("--all-features")])
        );
    }
}
