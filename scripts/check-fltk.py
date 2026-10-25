#!/usr/bin/env python3
"""Run the real FLTK adapter assertions under Xvfb (or an existing display)."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
subprocess.run(["cargo", "build", "--locked", "-p", "backend-settings",
                "--features", "fltk", "--example", "fltk_probe", "--example", "fltk_schedule_probe"], cwd=ROOT, check=True)
with tempfile.TemporaryDirectory(prefix="plico-fltk-") as tmp:
    env = dict(os.environ, PLICO_FLTK_PROBE_OUT=str(Path(tmp) / "settings"))
    subprocess.run([str(ROOT / "target/debug/examples/fltk_probe")],
                   cwd=ROOT, env=env, check=True, timeout=30)
    for name in ("light", "dark", "narrow", "overview", "details-mid", "details", "progress-mid", "activity"):
        data = (Path(tmp) / f"settings-{name}.ppm").read_bytes()
        magic, dimensions, depth, pixels = data.split(b"\n", 3)
        w, h = map(int, dimensions.split())
        assert magic == b"P6" and depth == b"255"
        assert len(pixels) == w * h * 3 and len(set(pixels)) > 16
subprocess.run([str(ROOT / "target/debug/examples/fltk_schedule_probe")], cwd=ROOT, check=True, timeout=15)
print("FLTK native callbacks, retention, removal, resize, theme and captures passed")
