#!/usr/bin/env python3
"""Check workspace members without recursively scanning vendor or reference crates."""
import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
    cwd=ROOT,
    text=True,
))
members = set(metadata["workspace_members"])
manifests = sorted(
    package["manifest_path"]
    for package in metadata["packages"]
    if package["id"] in members
)
if not manifests:
    sys.exit("No workspace members found")


def production_dependencies(manifest):
    dependencies = set(manifest.get("dependencies", {}))
    for target in manifest.get("target", {}).values():
        dependencies.update(target.get("dependencies", {}))
    return dependencies


# Keep the portable declaration stack and backend contracts below hosts. Cargo
# rejects cycles, but it does not stop a low layer from directly reaching into
# one higher layer and permanently reversing ownership.
forbidden = {
    "plico": {
        "plico-dev", "plico-renderer", "plico-runtime", "plico-app-host",
        "plico-wgpu", "plico-skia", "plico-shell", "plico-desktop",
    },
    "plico-motion": {
        "plico-dev", "plico-renderer", "plico-runtime", "plico-app-host",
        "plico-wgpu", "plico-skia", "plico-shell", "plico-desktop",
    },
    "plico-dev": {
        "plico-renderer", "plico-runtime", "plico-app-host", "plico-wgpu",
        "plico-skia", "plico-shell", "plico-desktop",
    },
    "plico-renderer": {
        "plico-dev", "plico-runtime", "plico-app-host", "plico-wgpu",
        "plico-skia", "plico-shell", "plico-desktop",
    },
    "plico-runtime": {
        "plico-dev", "plico-app-host", "plico-wgpu", "plico-skia",
        "plico-shell", "plico-desktop",
    },
    "plico-app-host": {"plico-wgpu", "plico-skia", "plico-shell", "plico-desktop"},
    "plico-wgpu": {"plico-app-host", "plico-skia", "plico-shell", "plico-desktop"},
    "plico-skia": {"plico-app-host", "plico-wgpu", "plico-shell", "plico-desktop"},
    "plico-shell": {"plico-dev", "plico-runtime", "plico-app-host", "plico-desktop"},
}
packages = {package["name"]: package for package in metadata["packages"]}
violations = []
for package, denied in forbidden.items():
    manifest_path = Path(packages[package]["manifest_path"])
    direct = production_dependencies(tomllib.loads(manifest_path.read_text()))
    for dependency in sorted(direct & denied):
        violations.append(f"{package} must not depend on higher layer {dependency}")
if violations:
    sys.exit("Workspace dependency layering failed:\n" + "\n".join(violations))
print("Workspace dependency layering holds.")

# Explicit manifest paths prevent machete's recursive directory walk from
# entering nested, excluded packages such as plico-desktop/reference.
sys.exit(subprocess.run(
    ["cargo-machete", "--with-metadata", *manifests], cwd=ROOT,
).returncode)
