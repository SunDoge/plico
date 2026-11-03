use std::{fs, path::Path, process::Command};

fn fixture(default: &str) -> tempfile::TempDir {
    let root = tempfile::Builder::new()
        .prefix("plico apps ")
        .tempdir()
        .unwrap();
    fs::write(root.path().join("Cargo.toml"), format!("[workspace]\nresolver='3'\nmembers=['apps/one','apps/two','helper']\ndefault-members=['{default}']\n")).unwrap();
    for name in ["one", "two"] {
        let directory = root.path().join("apps").join(name);
        fs::create_dir_all(directory.join("src/nested")).unwrap();
        fs::write(
            directory.join("Cargo.toml"),
            format!(
                r#"[package]
name='{name}'
version='0.1.0'
edition='2024'
[features]
default=['desktop']
desktop=[]
extra=[]
[package.metadata.plico]
host='missing-host'
"#
            ),
        )
        .unwrap();
        fs::write(directory.join("src/lib.rs"), "pub fn value() {}\n").unwrap();
        fs::write(directory.join("src/main.rs"), r#"fn main() {
println!("app={} desktop={} extra={} args={:?}", env!("CARGO_PKG_NAME"), cfg!(feature="desktop"), cfg!(feature="extra"), std::env::args().skip(1).collect::<Vec<_>>());
}"#).unwrap();
    }
    fs::create_dir_all(root.path().join("helper/src")).unwrap();
    fs::write(
        root.path().join("helper/Cargo.toml"),
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    fs::write(root.path().join("helper/src/lib.rs"), "").unwrap();
    root
}
fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_plico"));
    command
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"));
    command
}
#[test]
fn run_infers_directory_preserves_arguments_and_uses_normal_features() {
    let root = fixture("apps/two");
    let output = command(root.path())
        .args(["run", "apps/one", "--", "a b"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("app=one desktop=true extra=false"),
        "{stdout}"
    );
    assert!(stdout.contains("args=[\"a b\"]"));
    let output = command(root.path())
        .current_dir(root.path().join("apps/one/src/nested"))
        .arg("run")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("app=one desktop=true extra=false"));
    let output = command(root.path())
        .args(["run", "--no-default-features", "--features", "extra"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("app=two desktop=false extra=true"));
}
#[test]
fn dev_uses_directory_or_workspace_default_instead_of_gallery() {
    let root = fixture("apps/two");
    for (path, selected) in [(Some("apps/one"), "one"), (None, "two")] {
        let mut cmd = command(root.path());
        cmd.arg("dev");
        if let Some(path) = path {
            cmd.arg(path);
        }
        let output = cmd.arg("--no-watch").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains(&format!("app={selected} desktop=true extra=false"))
        );
    }
}
#[test]
fn ambiguous_workspace_and_non_application_directory_are_actionable_errors() {
    let root = fixture("helper");
    let output = command(root.path()).arg("dev").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("multiple applications"), "{stderr}");
    assert!(stderr.contains("apps/one") && stderr.contains("apps/two"));
    for mode in ["run", "dev"] {
        let output = command(root.path())
            .args([mode, "helper"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(stderr.contains("package helper"), "{stderr}");
    }
}

#[test]
fn dev_runs_normal_binary_without_module_exports() {
    let root = fixture("apps/two");
    let output = command(root.path())
        .args(["dev", "apps/one", "--no-watch", "--", "payload"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("app=one desktop=true extra=false"),
        "{stdout}"
    );
    assert!(stdout.contains("payload"));
}

#[test]
fn watch_keeps_old_process_on_build_failure_and_restarts_after_fix() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let root = fixture("apps/one");
    let record = root.path().join("process.txt");
    let stop = root.path().join("stop");
    let source = root.path().join("apps/one/src/main.rs");
    let code = |value: &str| {
        format!(
            r#"fn main() {{
        let args: Vec<_> = std::env::args().collect();
        std::fs::write(&args[1], format!("{{}}:{value}", std::process::id())).unwrap();
        while !std::path::Path::new(&args[2]).exists() {{ std::thread::sleep(std::time::Duration::from_millis(20)); }}
    }}"#
        )
    };
    fs::write(&source, code("before")).unwrap();
    let log_path = root.path().join("dev.log");
    let mut command = command(root.path());
    command
        .args(["dev", "apps/one", "--"])
        .arg(&record)
        .arg(&stop)
        .stdout(std::process::Stdio::null())
        .stderr(fs::File::create(&log_path).unwrap());
    struct Cleanup {
        child: std::process::Child,
        stop: std::path::PathBuf,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::write(&self.stop, "stop");
            for _ in 0..100 {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    let _cleanup = Cleanup {
        child: command.spawn().unwrap(),
        stop,
    };
    let wait = |condition: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !condition() {
            assert!(
                Instant::now() < deadline,
                "{}",
                fs::read_to_string(&log_path).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&|| {
        fs::read_to_string(&record)
            .unwrap_or_default()
            .ends_with(":before")
    });
    let before = fs::read_to_string(&record).unwrap();
    fs::write(&source, "fn main() { syntax error }").unwrap();
    wait(&|| {
        fs::read_to_string(&log_path)
            .unwrap_or_default()
            .contains("build failed for one")
    });
    assert_eq!(fs::read_to_string(&record).unwrap(), before);
    fs::write(&source, code("after")).unwrap();
    wait(&|| {
        fs::read_to_string(&record)
            .unwrap_or_default()
            .ends_with(":after")
    });
    let after = fs::read_to_string(&record).unwrap();
    assert_ne!(before.split(':').next(), after.split(':').next());
}
