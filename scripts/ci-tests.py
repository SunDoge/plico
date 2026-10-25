#!/usr/bin/env python3
"""Run a native test shard; fail if workspace packages lack a test assignment."""
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
GROUPS = {
    "core": ["plico-cli", "plico-dev", "plico-core", "plico-macros", "plico-editor",
             "plico-motion", "plico-markdown", "plico-native", "plico-terminal-widget", "plico-web-demo"],
    "rendering": ["plico-renderer", "plico-shell", "plico-runtime", "plico-vello"],
    "apps": ["plico-gallery", "plico-web-host", "plico-launcher", "plico-services-demo", "plico-terminal"],
}
# These packages have dedicated backend jobs, or require physical ESP hardware.
SEPARATE = {"plico-fltk", "plico-lvgl", "plico-lvgl-esp32p4", "plico-skia", "plico-backend-settings"}
FEATURES = {
    "core": ["--all-features"],
    # Skia is tested in its own job; do not pull its native build into every shard.
    "rendering": ["--features", "plico-vello/hot-reload,plico-vello/profiling,plico-runtime/profiling,plico-shell/profiling,plico-renderer/profiling"],
    "apps": ["--all-features"],
}


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT))
    members = set(metadata["workspace_members"])
    packages = {p["name"] for p in metadata["packages"] if p["id"] in members}
    assigned = [p for group in GROUPS.values() for p in group] + sorted(SEPARATE)
    if len(assigned) != len(set(assigned)) or set(assigned) != packages:
        sys.exit(f"Fix CI test assignments: missing={packages - set(assigned)}, stale={set(assigned) - packages}, duplicates={len(assigned) != len(set(assigned))}")
    if sys.argv[1:] == ["--check"]:
        print("Every workspace package has a CI test assignment.")
        return
    if len(sys.argv) != 2 or sys.argv[1] not in GROUPS:
        sys.exit("usage: ci-tests.py core|rendering|apps|--check")
    group = sys.argv[1]
    command = ["cargo", "test", "--locked"]
    for package in GROUPS[group]:
        command.extend(["-p", package])
    command.extend(FEATURES[group])
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
