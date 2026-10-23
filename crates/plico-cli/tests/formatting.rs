//! Exercise the installed CLI with real Cargo metadata and rustfmt, without GUI dependencies.
use std::{fs, process::Command};
const SOURCE: &str = r#"#[component]
#[rustfmt::skip]
fn view(cx: &mut UiContext) {
    column(cx)
      .gap(8) + |cx| {
        text(cx,"hello");
    };
}
fn ordinary( ) {let n=1;}
"#;
fn fixture() -> tempfile::TempDir {
    let directory = tempfile::Builder::new()
        .prefix("plico formatter ")
        .tempdir()
        .unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname='formatter-fixture'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    fs::write(directory.path().join("src/lib.rs"), SOURCE).unwrap();
    directory
}
fn command(directory: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_plico"));
    command.arg("-C").arg(directory.path()).arg("fmt");
    command
}
#[test]
fn check_is_read_only_write_is_idempotent_and_explicit_paths_work() {
    let directory = fixture();
    let path = directory.path().join("src/lib.rs");
    let output = command(&directory).arg("--check").output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("would format"));
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
    let output = command(&directory)
        .args(["--width", "80", "src/lib.rs"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let once = fs::read_to_string(&path).unwrap();
    assert!(once.contains("column(cx).gap(8) + |cx| {"));
    assert!(
        command(&directory)
            .args(["--check", "src/lib.rs"])
            .status()
            .unwrap()
            .success()
    );
    assert!(command(&directory).status().unwrap().success());
    assert_eq!(fs::read_to_string(&path).unwrap(), once);
}
#[test]
fn failure_in_another_file_leaves_every_file_untouched() {
    let directory = fixture();
    fs::write(directory.path().join("src/z_invalid.rs"), "fn invalid( {").unwrap();
    let output = command(&directory).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid Rust"));
    assert_eq!(
        fs::read_to_string(directory.path().join("src/lib.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("src/z_invalid.rs")).unwrap(),
        "fn invalid( {"
    );
}
#[test]
fn package_selection_and_rustfmt_configuration_are_respected() {
    let directory = fixture();
    fs::write(directory.path().join("rustfmt.toml"), "hard_tabs = true\n").unwrap();
    let output = command(&directory)
        .args(["-p", "formatter-fixture"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = fs::read_to_string(directory.path().join("src/lib.rs")).unwrap();
    assert!(source.contains("\tlet n = 1;"));
    assert!(
        command(&directory)
            .args(["--check", "-p", "formatter-fixture"])
            .status()
            .unwrap()
            .success()
    );
    let output = command(&directory)
        .args(["-p", "missing"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no workspace package"));
}
