#!/usr/bin/env python3
"""Exercise Git-only releases in a disposable snapshot; never push or publish."""
import json
from pathlib import Path
import subprocess
import sys
import shutil
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
config = tomllib.loads((ROOT / "release-plz.toml").read_text())
workspace = config["workspace"]
assert workspace["git_only"] is True
assert workspace.get("publish", False) is False
assert workspace["git_release_enable"] is False
assert workspace["git_tag_enable"] is False
assert workspace["release_always"] is False
assert workspace["git_tag_name"] == "v{{ version }}"
assert [p["name"] for p in config["package"] if p.get("git_tag_enable")] == ["plico-cli"]
assert [p["name"] for p in config["package"] if p.get("git_release_enable")] == ["plico-cli"]
assert next(p for p in config["package"] if p["name"] == "plico-cli")["git_release_name"] == "Plico v{{ version }}"


def run(args, cwd):
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"{args!r}\n{result.stdout}\n{result.stderr}")
    return result.stdout


with tempfile.TemporaryDirectory(prefix="plico-release-") as directory:
    checkout = Path(directory)
    # Validate the current publishable tree, including uncommitted additions and
    # deletions. Ignored reports/build outputs and .git never enter the fixture.
    paths = subprocess.check_output([
        "git", "ls-files", "--cached", "--others", "--exclude-standard", "-z",
    ], cwd=ROOT).decode().split("\0")
    for name in paths:
        if not name:
            continue
        source = ROOT / name
        if not source.is_file() and not source.is_symlink():
            continue
        destination = checkout / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination, follow_symlinks=False)
    metadata = json.loads(run([
        "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
    ], checkout))
    members = set(metadata["workspace_members"])
    packages = {p["name"] for p in metadata["packages"] if p["id"] in members}
    includes = next(p["changelog_include"] for p in config["package"] if p["name"] == "plico-cli")
    assert set(includes) == packages - {"plico-cli"}, "Keep changelog_include aligned with workspace members"
    run(["git", "init", "-q"], checkout)
    run(["git", "config", "user.name", "Release fixture"], checkout)
    run(["git", "config", "user.email", "release@example.invalid"], checkout)
    run(["git", "config", "commit.gpgsign", "false"], checkout)
    run(["git", "config", "tag.gpgsign", "false"], checkout)
    run(["git", "remote", "add", "origin", "https://github.com/SunDoge/plico.git"], checkout)

    def commit(message):
        run(["git", "add", "."], checkout)
        run(["git", "commit", "-qm", message], checkout)

    def version():
        return tomllib.loads((checkout / "Cargo.toml").read_text())["workspace"]["package"]["version"]

    def update():
        run(["release-plz", "update"], checkout)
        run([sys.executable, "scripts/check-versions.py"], checkout)

    commit("feat: initial release fixture")
    initial = version()
    update()
    assert version() == initial, "Initial release should preserve the declared version"
    commit(f"chore: release v{initial}")
    run(["git", "tag", f"v{initial}"], checkout)
    # This backend is outside the CLI dependency chain: its changes must still
    # bump the common version and appear in the repository changelog.
    backend = checkout / "crates/plico-lvgl/src/lib.rs"
    backend.write_text(backend.read_text() + "\n// Release fixture.\n")
    commit("fix(lvgl): exercise independent backend release")
    update()
    major, minor, patch = map(int, initial.split("."))
    assert version() == f"{major}.{minor}.{patch + 1}"
    assert "exercise independent backend release" in (checkout / "CHANGELOG.md").read_text()
    assert run(["git", "tag", "--list"], checkout).splitlines() == [f"v{initial}"]
    changes = set(run(["git", "diff", "--name-only"], checkout).splitlines())
    assert changes == {"Cargo.toml", "Cargo.lock", "CHANGELOG.md"}, changes
    commit(f"chore: release v{version()}")
    run(["git", "tag", f"v{version()}"], checkout)
    update()
    assert not run(["git", "status", "--porcelain"], checkout), "No-change update must be stable"
print("Git-only release: initial version, shared patch bump, changelog and no-change stability passed.")
