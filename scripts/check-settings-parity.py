#!/usr/bin/env python3
"""Cross-backend pixel parity for the shared settings app.

Drives the settings app with --backend vello and --backend skia through the same route/theme state
machine on one Xvfb display and captures each state twice. The assertion is
self-calibrating rather than a magic absolute tolerance: rendering the *same*
state on both backends must be closer (RMSE) than rendering *different* states
on the same backend by a wide factor. This detects large visual drift, without
proving structural equivalence or pretending rasterizers are pixel-identical. Results land in reports/settings-parity/parity.json.
"""
import json
import os
import re
import subprocess
from pathlib import Path

from plico_smoke import Xvfb, WindowApp

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
REPORT = ROOT / "reports/settings-parity"
# (name, clicks applied after the previous state) — identical for both backends.
STATES = [
    ("overview", []),
    ("details", [(1030, 97)]),
    ("activity", [(80, 142)]),
    ("preferences", [(80, 182)]),
    ("dark", [(1000, 34)]),
]
SAME_STATE_MAX_FACTOR = 0.5  # same-state cross-backend RMSE must stay under half the baseline

subprocess.run(
    ["cargo", "build", "--locked", "-p", "plico-backend-settings", "--features", "skia,vello", "--bins"],
    cwd=ROOT, check=True,
)
REPORT.mkdir(parents=True, exist_ok=True)


def drive(backend, server):
    env = server.env(
        PLICO_SKIA_DEVICE="llvmpipe" if backend == "skia" else None,
        PLICO_COMPARE_VELLO=None,
        PLICO_PERFORMANCE_HUD=None,  # HUD text timings would add pixel noise
        WGPU_BACKEND="vulkan",
    )
    captures = {}
    with WindowApp(TARGET / "debug" / "plico-backend-settings", env, REPORT / f"{backend}.log", args=["--backend", backend]) as app:
        app.window(f"no {backend} settings window")
        app.focus()
        app.wait_for_frame(REPORT / f"{backend}-ready.png")
        for name, clicks in STATES:
            for x, y in clicks:
                app.click(x, y)
            captures[name] = app.capture_hash(REPORT / f"{backend}-{name}.png")
        app.assert_log_clean()
    return captures


def rmse(a, b):
    result = subprocess.run(
        ["compare", "-metric", "RMSE", str(a), str(b), "null:"],
        capture_output=True, text=True, timeout=30,
    )
    if result.returncode not in (0, 1):
        raise RuntimeError(f"ImageMagick compare failed: {result.stderr}")
    output = result.stderr
    match = re.search(r"\(([-0-9.eE]+)\)", output)
    if match is None:
        raise RuntimeError(f"ImageMagick compare failed for {a} vs {b}: {output}")
    return float(match.group(1))


with Xvfb() as server:
    vello = drive("vello", server)
    skia = drive("skia", server)

# Same state across backends: the parity signal. Different states on Vello: the
# baseline that separates "same UI, other rasterizer" from "different UI".
same_state = {name: rmse(REPORT / f"vello-{name}.png", REPORT / f"skia-{name}.png") for name, _ in STATES}
baseline_pairs = [("overview", "activity"), ("overview", "preferences"), ("details", "dark"), ("preferences", "dark")]
baseline = {f"{a}~{b}": rmse(REPORT / f"vello-{a}.png", REPORT / f"vello-{b}.png") for a, b in baseline_pairs}

report = {
    "same_state_cross_backend_rmse": same_state,
    "different_state_baseline_rmse": baseline,
    "threshold": SAME_STATE_MAX_FACTOR * min(baseline.values()),
}
(REPORT / "parity.json").write_text(json.dumps(report, indent=2) + "\n")

worst = max(same_state, key=same_state.get)
assert same_state[worst] < report["threshold"], (
    f"cross-backend parity broke at {worst}: RMSE {same_state[worst]:.4f} "
    f"vs threshold {report['threshold']:.4f} (half the different-state minimum)"
)
print(json.dumps(report, indent=2))
print(f"PASS: same-state cross-backend RMSE ({same_state[worst]:.4f} worst, '{worst}') "
      f"stays well under half the different-state baseline; captures in reports/settings-parity")
