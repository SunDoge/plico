#!/usr/bin/env python3
"""Linux/X11 warm-process comparison; run under xvfb-run after building both bins.

Window visibility is a coarse startup proxy, not input-to-photon measurement.
RSS includes shared pages and GPU driver allocations. No binary is stripped here.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--profile", default="release", choices=("release", "debug"))
p.add_argument("--runs", type=int, default=3)
p.add_argument("--out", type=Path, default=ROOT / "reports/fltk/comparison")
args = p.parse_args()
if args.runs < 1:
    p.error("--runs must be positive")
args.out.mkdir(parents=True, exist_ok=True)

def cpu_ticks(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[11]) + int(fields[12])

def inspect(backend):
    binary = ROOT / "target" / args.profile / f"settings-{backend}"
    env = dict(os.environ, WINIT_UNIX_BACKEND="x11")
    env.pop("WAYLAND_DISPLAY", None)
    with (args.out / f"{backend}.log").open("w") as log:
        start = time.monotonic()
        process = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
        try:
            window = None
            while time.monotonic() - start < 30:
                if process.poll() is not None:
                    raise RuntimeError(f"{backend} exited early; see {backend}.log")
                result = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid)],
                                        capture_output=True, text=True)
                if result.returncode == 0:
                    window = result.stdout.splitlines()[0]
                    break
                time.sleep(.01)
            if window is None:
                raise RuntimeError(f"no {backend} window after 30 seconds")
            visible_ms = (time.monotonic() - start) * 1000
            time.sleep(.5)
            status = Path(f"/proc/{process.pid}/status").read_text().splitlines()
            rss = int(next(line.split()[1] for line in status if line.startswith("VmRSS:")))
            before = cpu_ticks(process.pid)
            time.sleep(1)
            idle_ms = (cpu_ticks(process.pid) - before) / os.sysconf("SC_CLK_TCK") * 1000
            if shutil.which("import"):
                subprocess.run(["import", "-window", window, str(args.out / f"{backend}.png")], check=True, timeout=10)
            return {"binary_bytes": binary.stat().st_size, "window_visible_ms": round(visible_ms, 2),
                    "rss_kib": rss, "idle_cpu_ms_per_second": round(idle_ms, 2)}
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

results = {}
for backend in ("fltk", "vello"):
    samples = [inspect(backend) for _ in range(args.runs)]
    results[backend] = {key: statistics.median(sample[key] for sample in samples)
                        for key in samples[0]}
    results[backend]["samples"] = samples
(args.out / "measurements.json").write_text(json.dumps(results, indent=2) + "\n")
print(json.dumps(results, indent=2))
