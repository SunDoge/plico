#!/usr/bin/env python3
"""Build first, then run sequential CPU probes without compiler contention.

No window/GPU or hot-reload transport is measured. Output is a local report.
"""
import json
import os
import shutil
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CASES = [
    ("plico", "memo_probe", "core"),
    ("plico", "declaration_churn", "churn"),
    ("plico-markdown", "parse_cache_probe", "markdown"),
    ("plico-desktop", "memo_backend_probe", "backend"),
]


def main():
    report = Path(os.environ.get("PLICO_REPORT_DIR", ROOT / "reports/memo-experiments"))
    report.mkdir(parents=True, exist_ok=True)
    command = ["cargo", "build", "--release"]
    for package, example, _ in CASES:
        command.extend(["-p", package, "--example", example])
    subprocess.run(command, cwd=ROOT, check=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT
    ))
    executables = Path(metadata["target_directory"]) / "release/examples"
    suffix = ".exe" if os.name == "nt" else ""
    snapshot = os.environ.get("PLICO_PROBE_SNAPSHOT_DIR")
    if snapshot:
        Path(snapshot).mkdir(parents=True, exist_ok=True)
        for _, example, _ in CASES:
            shutil.copy2(executables / (example + suffix), Path(snapshot) / (example + suffix))
    baseline = os.environ.get("PLICO_BASELINE_BIN_DIR")
    for round_number in range(1, 4):
        for _, example, name in CASES:
            runs = [("after", executables / (example + suffix))]
            if baseline:
                runs.insert(0, ("before", Path(baseline) / (example + suffix)))
                if round_number % 2 == 0:
                    runs.reverse()
            for label, executable in runs:
                output = subprocess.check_output([executable], cwd=ROOT)
                filename = f"{name}-{label}-{round_number}.csv" if baseline else f"{name}-{round_number}.csv"
                (report / filename).write_bytes(output)
    print(f"Wrote three sequential rounds to {report}")


if __name__ == "__main__":
    main()
