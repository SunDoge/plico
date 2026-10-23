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
[lib]
crate-type=['rlib','cdylib']
[features]
default=['desktop']
desktop=[]
hot-reload=[]
[package.metadata.plico]
host='missing-host'
"#
            ),
        )
        .unwrap();
        fs::write(directory.join("src/lib.rs"), "pub fn value() {}\n").unwrap();
        fs::write(directory.join("src/main.rs"), r#"fn main() {
println!("app={} desktop={} hot={} module={} args={:?}", env!("CARGO_PKG_NAME"), cfg!(feature="desktop"), cfg!(feature="hot-reload"), std::env::var_os("PLICO_DEV_MODULE").is_some(), std::env::args().skip(1).collect::<Vec<_>>());
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
        .env("PLICO_DEV_MODULE", "stale descriptor")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("app=one desktop=true hot=false module=false"),
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
    assert!(String::from_utf8_lossy(&output.stdout).contains("app=one desktop=true hot=false"));
    let output = command(root.path())
        .args(["run", "--no-default-features", "--features", "hot-reload"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("app=two desktop=false hot=true"));
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
        let output = cmd.output().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("missing-host was not found in package {selected}")),
            "{stderr}"
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
fn dev_enables_host_hot_reload_and_publishes_a_separate_module() {
    let root = fixture("apps/two");
    let manifest = root.path().join("apps/one/Cargo.toml");
    let source = fs::read_to_string(&manifest)
        .unwrap()
        .replace("host='missing-host'", "host='one'");
    fs::write(manifest, source).unwrap();
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
        stdout.contains("app=one desktop=true hot=true module=true"),
        "{stdout}"
    );
    assert!(stdout.contains("args=[\"payload\"]"));
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(root.path().join("target/plico-dev/one/build-report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["package"], "one");
}
