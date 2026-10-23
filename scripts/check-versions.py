#!/usr/bin/env python3
"""Require every Cargo workspace member to inherit the common release version."""
import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
with (ROOT / "Cargo.toml").open("rb") as source:
    version = tomllib.load(source)["workspace"]["package"]["version"]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
    cwd=ROOT,
    text=True,
))
members = set(metadata["workspace_members"])
errors = []
for package in metadata["packages"]:
    if package["id"] not in members:
        continue
    manifest = Path(package["manifest_path"])
    with manifest.open("rb") as source:
        declared = tomllib.load(source)["package"].get("version")
    if declared != {"workspace": True} or package["version"] != version:
        errors.append(f"{manifest.relative_to(ROOT)}: use version.workspace = true ({version})")
if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print(f"All {len(members)} workspace packages inherit version {version}.")
