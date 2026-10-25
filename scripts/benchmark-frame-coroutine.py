#!/usr/bin/env python3
"""Run the isolated release simulation repeatedly; artifacts stay in reports/."""
import csv
import json
import os
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parent.parent
REPORTS = ROOT / "reports" / "frame-coroutine"


def main():
    runs = int(os.environ.get("PLICO_BENCH_RUNS", "3"))
    if not 1 <= runs <= 20:
        raise SystemExit("PLICO_BENCH_RUNS must be between 1 and 20")
    REPORTS.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "plico-core",
                    "--example", "frame_coroutine"], cwd=ROOT, check=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT))
    binary = Path(metadata["target_directory"]) / "release" / "examples" / "frame_coroutine"
    env = dict(os.environ, PLICO_COROUTINE_BENCH="1")
    # Pin benchmark children to one allowed CPU to reduce heterogeneous-core migration.
    cpu = None
    if hasattr(os, "sched_getaffinity"):
        cpu = int(os.environ.get("PLICO_BENCH_CPU", min(os.sched_getaffinity(0))))
        os.sched_setaffinity(0, {cpu})
    (REPORTS / "environment.json").write_text(json.dumps({
        "runs": runs, "cpu_affinity": cpu,
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "cpu_model": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                           if line.startswith("model name")), "unknown") if Path("/proc/cpuinfo").exists() else "unknown",
    }, indent=2) + "\n")
    groups = {}
    for index in range(runs):
        output = REPORTS / f"run-{index + 1}.csv"
        with output.open("w") as stream:
            subprocess.run([str(binary)], cwd=ROOT, env=env, stdout=stream, check=True)
        with output.open() as stream:
            rows = list(csv.DictReader(stream))
        if len(rows) != 168:
            raise RuntimeError(f"unexpected scenario count: {len(rows)}")
        for row in rows:
            key = tuple(row[k] for k in ("model", "components", "work_rounds", "scenario"))
            groups.setdefault(key, []).append(row)
        print(f"completed run {index + 1}/{runs}", flush=True)
    summary = []
    for key, samples in groups.items():
        assert len(samples) == runs
        item = dict(zip(("model", "components", "work_rounds", "scenario"), key))
        for field in ("median_ns", "p95_ns", "alloc_calls", "allocated_bytes",
                      "shared_nodes_visited", "business_components_executed"):
            item[field] = statistics.median(float(s[field]) for s in samples)
        item["median_ns_min"] = min(int(s["median_ns"]) for s in samples)
        item["median_ns_max"] = max(int(s["median_ns"]) for s in samples)
        summary.append(item)
    (REPORTS / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("1000 components; median of per-run medians (us); synthetic work is NOT a real app workload")
    print("rounds scenario           immediate matched coroutine cached ratio(coroutine/immediate)")
    lookup = {(r["model"], r["work_rounds"], r["scenario"]): r for r in summary
              if r["components"] == "1000"}
    for rounds in ("0", "1024", "8192"):
        for scenario in ("sparse-one", "dense-all", "click-one"):
            values = [lookup[(model, rounds, scenario)]["median_ns"] / 1000
                      for model in ("immediate", "immediate-matched-shape", "coroutine-full-replay", "immediate-cached-work")]
            print(f"{rounds:>6} {scenario:18} {values[0]:8.1f} {values[1]:8.1f} {values[2]:10.1f} {values[3]:8.1f} {values[2]/values[0]:.2f}x")
    print(f"raw data and summary: {REPORTS}")


if __name__ == "__main__":
    main()
