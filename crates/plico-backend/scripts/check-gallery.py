#!/usr/bin/env python3
"""Real Wabou captures and reference behavior, no desktop input injection."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
WABOU = Path(os.environ.get("WABOU_ROOT", str(ROOT.parents[2] / "wabou")))
REPORTS = ROOT / "reports"
REPORTS.mkdir(exist_ok=True)
REFERENCE = ROOT / "reference"
if not (REFERENCE / "node_modules").is_dir():
    raise SystemExit(
        "Optional legacy Solid comparison requires dependencies installed locally in "
        "reference/node_modules; this script no longer links to Wabou node_modules. "
        "For the maintained Rust gallery, run: cargo run -p plico-gallery"
    )
CLI = WABOU / "target/debug/wabou"

def run(*args):
    subprocess.run([str(a) for a in args], cwd=WABOU, check=True)

def solid(name, actions=(), build=False):
    args = [CLI, "render", REFERENCE, "--out", REPORTS / f"{name}.png", "--width", 920,
            "--height", 780, "--wait-ms", 200, "--snapshot", REPORTS / f"{name}-tree.json"]
    if not build:
        args.append("--skip-build")
    run(*args, *actions)
    return json.loads((REPORTS / f"{name}-tree.json").read_text())

def point(tree, label):
    node = next((n for n in tree["nodes"] if dict(n["attrs"]).get("aria-label") == label), None)
    if node is None:
        node = next(n for n in tree["nodes"] if n.get("text") == label)
    r = node["rect"]
    return ["--click", str(r["x"] + r["width"] / 2), str(r["y"] + r["height"] / 2)]

base = solid("solid-light", build=True)
dark = solid("solid-dark", point(base, "Dark theme") + point(base, "Large text"))
changed = solid("solid-interacted", point(base, "Increment") + point(base, "Notifications") + point(base, "Save automatically"))
text = " ".join(n["text"] for n in changed["nodes"] if n.get("text"))
assert "Clicked 1 times" in text, text
assert "Notifications are enabled" in text
assert "Changes will be saved automatically." in text
assert next(dict(n["attrs"])["aria-checked"] for n in dark["nodes"] if dict(n["attrs"]).get("aria-label") == "Dark theme") == "true"
for theme, flags in [("light", []), ("dark", ["--dark", "--large"])]:
    run("cargo", "run", "--quiet", "--manifest-path", ROOT / "Cargo.toml", "--target-dir", WABOU / "target", "--", "render", "--out", REPORTS / f"rust-{theme}.png", *flags)
result = subprocess.run(["cargo", "run", "--quiet", "--manifest-path", str(ROOT / "Cargo.toml"), "--target-dir", str(WABOU / "target"), "--bin", "probe"], cwd=WABOU, check=True, capture_output=True, text=True)
(REPORTS / "rust-work.json").write_text(result.stdout)
summary = {"solid_reference_behavior": "passed", "solid_light_nodes": len(base["nodes"]), "rust_nodes": json.loads(result.stdout)["frames"][0]["nodes"],
           "note": "Different component decomposition; node counts are not a speed comparison. Wabou CLI --metrics is currently unsupported."}
(REPORTS / "comparison.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
