#!/usr/bin/env python3
"""Check workspace members without recursively scanning vendor or reference crates."""
import json
from pathlib import Path
import subprocess
import sys

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
# Static app builds must not pull in the optional native module ABI. Keep this
# separate from all-features CI, whose feature unification intentionally enables it.
for package in ("gallery", "services-demo"):
    tree = subprocess.check_output([
        "cargo", "tree", "--locked", "-p", package, "--edges", "normal", "--prefix", "none", "--format", "{p}",
    ], cwd=ROOT, text=True)
    if any(line.startswith(("stabby ", "stabby-abi ", "stabby-macros ")) for line in tree.splitlines()):
        sys.exit(f"Static {package} unexpectedly depends on stabby")
print("Static applications do not depend on stabby.")
# Explicit manifest paths prevent machete's recursive directory walk from
# entering nested, excluded packages such as plico-vello/reference.
sys.exit(subprocess.run(
    ["cargo-machete", "--with-metadata", *manifests], cwd=ROOT,
).returncode)
