#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "plico cli {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join("crates/plico")).unwrap();
        fs::create_dir_all(p.join("crates/plico-cli")).unwrap();
        for path in [
            "Cargo.toml",
            "crates/plico/Cargo.toml",
            "crates/plico-cli/Cargo.toml",
        ] {
            fs::write(p.join(path), "").unwrap();
        }
        fs::write(p.join("cargo"),"#!/bin/sh\nif [ \"$1\" = locate-project ]; then printf '%s/Cargo.toml\\n' \"$PLICO_TEST_ROOT\"; else printf '%s\\n' \"$@\" > \"$PLICO_TEST_ROOT/args\"; exit \"${PLICO_TEST_EXIT:-0}\"; fi\n").unwrap();
        fs::set_permissions(p.join("cargo"), fs::Permissions::from_mode(0o755)).unwrap();
        Self(p)
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_plico"));
        c.current_dir(&self.0)
            .env("PLICO_CARGO", self.0.join("cargo"))
            .env("PLICO_TEST_ROOT", &self.0);
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn help_does_not_need_a_workspace_or_cargo() {
    let output = Command::new(env!("CARGO_BIN_EXE_plico"))
        .arg("--help")
        .env("PLICO_CARGO", "/does/not/exist")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("render"));
}
#[test]
fn cargo_failure_is_propagated() {
    let f = Fixture::new();
    assert_eq!(
        f.command()
            .args(["check"])
            .env("PLICO_TEST_EXIT", "7")
            .status()
            .unwrap()
            .code(),
        Some(7)
    );
}
#[test]
fn render_path_with_spaces_is_one_argument() {
    let f = Fixture::new();
    assert!(
        f.command()
            .args(["render", "--out", "a b/$(hello).png"])
            .status()
            .unwrap()
            .success()
    );
    let a = fs::read_to_string(f.0.join("args")).unwrap();
    assert!(a.contains("--example\ncapture\n"));
    assert!(a.contains("--\n--out\na b/$(hello).png\n"));
}
#[test]
fn clippy_arguments_are_forwarded() {
    let f = Fixture::new();
    assert!(
        f.command()
            .args(["clippy", "--", "-D", "warnings"])
            .status()
            .unwrap()
            .success()
    );
    let a = fs::read_to_string(f.0.join("args")).unwrap();
    assert!(a.ends_with("--\n-D\nwarnings\n"));
}
#[test]
fn missing_cargo_is_reported() {
    let f = Fixture::new();
    let o = f
        .command()
        .arg("check")
        .env("PLICO_CARGO", "/does/not/exist")
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8(o.stderr).unwrap().contains("plico:"));
}

#[test]
fn path_wrapper_is_used_even_when_parent_cargo_injects_its_binary() {
    let f = Fixture::new();
    let mut paths = vec![f.0.clone()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let status = f
        .command()
        .args(["check"])
        .env_remove("PLICO_CARGO")
        .env("CARGO", "/does/not/exist/toolchain/cargo")
        .env("PATH", std::env::join_paths(paths).unwrap())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        fs::read_to_string(f.0.join("args"))
            .unwrap()
            .starts_with("check\n")
    );
}
