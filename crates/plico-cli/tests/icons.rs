use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_plico"))
        .current_dir(dir)
        .env("PLICO_CARGO", "/not-a-cargo-binary")
        .args(args)
        .output()
        .unwrap()
}
fn ok(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn source(dir: &Path) {
    fs::create_dir_all(dir.join("source/icons")).unwrap();
    let rust = include_str!("../../plico/src/icons.rs");
    for (name, constant) in [("check", "CHECK"), ("x", "X")] {
        let prefix = format!("pub const {constant}: SvgAsset = SvgAsset::new(\n    r#\"");
        let xml = rust
            .split_once(&prefix)
            .unwrap()
            .1
            .split_once("\"#,")
            .unwrap()
            .0;
        fs::write(dir.join(format!("source/icons/{name}.svg")), xml).unwrap();
    }
    fs::write(
        dir.join("source/LICENSE"),
        include_bytes!("../../plico/assets/lucide/LICENSE"),
    )
    .unwrap();
}

#[test]
fn add_merges_deterministically_and_check_works_without_cargo_or_source_cache() {
    let dir = tempfile::tempdir().unwrap();
    source(dir.path());
    let base = [
        "icons",
        "add",
        "lucide:check",
        "--source",
        "source",
        "--output",
        "my app/icons.rs",
        "--rust-crate",
        "plico",
    ];
    ok(cli(dir.path(), &base));
    let mut add = base;
    add[2] = "lucide:x";
    ok(cli(dir.path(), &add));
    let output = dir.path().join("my app/icons.rs");
    let before = fs::read(&output).unwrap();
    let rust = String::from_utf8(before.clone()).unwrap();
    assert!(rust.contains("pub const CHECK:"));
    assert!(rust.contains("pub const X:"));
    assert!(!rust.contains("pub const SAVE:"));
    let manifest = dir.path().join("my app/icons.rs.json");
    let metadata = fs::read(&manifest).unwrap();
    fs::remove_dir_all(dir.path().join("source")).unwrap();
    ok(cli(
        dir.path(),
        &[
            "icons",
            "add",
            "lucide:x",
            "--output",
            "my app/icons.rs",
            "--rust-crate",
            "plico",
        ],
    ));
    assert_eq!(fs::read(&output).unwrap(), before);
    assert_eq!(fs::read(&manifest).unwrap(), metadata);
    ok(cli(
        dir.path(),
        &["icons", "check", "--output", "my app/icons.rs"],
    ));
    fs::write(&output, rust.replace("<svg", "<bad")).unwrap();
    assert!(
        !cli(
            dir.path(),
            &["icons", "check", "--output", "my app/icons.rs"]
        )
        .status
        .success()
    );
    assert!(!cli(dir.path(), &base).status.success());
}

#[test]
fn invalid_names_bad_hash_and_handwritten_output_are_not_written() {
    let dir = tempfile::tempdir().unwrap();
    source(dir.path());
    for name in [
        "lucide:../../check",
        "other:check",
        "lucide:not-a-real-plico-icon",
    ] {
        assert!(
            !cli(dir.path(), &["icons", "add", name, "--source", "source"])
                .status
                .success()
        );
        assert!(!dir.path().join("src/icons.rs").exists());
    }
    fs::write(dir.path().join("source/icons/check.svg"), "corrupt").unwrap();
    assert!(
        !cli(
            dir.path(),
            &["icons", "add", "lucide:check", "--source", "source"]
        )
        .status
        .success()
    );
    assert!(!dir.path().join("src/icons.rs").exists());
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/icons.rs"), "// handwritten\n").unwrap();
    assert!(
        !cli(
            dir.path(),
            &["icons", "add", "lucide:x", "--source", "source"]
        )
        .status
        .success()
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src/icons.rs")).unwrap(),
        "// handwritten\n"
    );
}

#[test]
fn license_and_manifest_tampering_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    source(dir.path());
    ok(cli(
        dir.path(),
        &["icons", "add", "lucide:check", "--source", "source"],
    ));
    let license = dir.path().join("src/icons.rs.LICENSE");
    let original = fs::read(&license).unwrap();
    fs::write(&license, "bad license").unwrap();
    assert!(!cli(dir.path(), &["icons", "check"]).status.success());
    fs::write(license, original).unwrap();
    let manifest = dir.path().join("src/icons.rs.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    data["commit"] = "different pin".into();
    fs::write(manifest, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(!cli(dir.path(), &["icons", "check"]).status.success());
}
