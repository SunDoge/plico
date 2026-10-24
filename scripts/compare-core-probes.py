#!/usr/bin/env python3
"""Compare prebuilt core probes; alternate runs on one CPU to reduce bias.

Build both revisions with identical compiler/profile/features first. Pass their
release/examples directories. Keep the source baseline fixed: concurrent widget
or enum changes make the comparison invalid. Outputs are local report artifacts.
"""
import argparse
import csv
import io
import json
import os
from pathlib import Path
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--rounds", type=int, default=6)
    parser.add_argument("--out", type=Path, default=Path("reports/core-probes"))
    args = parser.parse_args()
    if args.rounds < 2:
        parser.error("at least two rounds are required")
    probes = ("memo_probe", "declaration_churn", "allocations")
    directories = {"before": args.before.resolve(), "after": args.after.resolve()}
    for directory in directories.values():
        for probe in probes:
            if not (directory / probe).is_file():
                parser.error(f"missing executable: {directory / probe}")
    cpu = None
    if hasattr(os, "sched_getaffinity"):
        cpu = min(os.sched_getaffinity(0))
        os.sched_setaffinity(0, {cpu})
    args.out.mkdir(parents=True, exist_ok=True)
    runs = {probe: {version: [] for version in directories} for probe in probes}
    for iteration in range(args.rounds):
        order = ("before", "after") if iteration % 2 == 0 else ("after", "before")
        for version in order:
            for probe in probes:
                result = subprocess.run([str(directories[version] / probe)],
                                        check=True, text=True, capture_output=True)
                (args.out / f"{version}-{probe}-{iteration}.txt").write_text(result.stdout)
                runs[probe][version].append(result.stdout)
    summary = []
    for probe, field, keys in (("memo_probe", "total_p50_us", ("case", "rows")),
                               ("declaration_churn", "p50_us", ("mode",))):
        cases = {}
        for version, outputs in runs[probe].items():
            for output in outputs:
                for row in csv.DictReader(io.StringIO(output)):
                    name = ":".join(row[key] for key in keys)
                    cases.setdefault(name, {}).setdefault(version, []).append(float(row[field]))
        for name, values in cases.items():
            before = statistics.median(values["before"])
            after = statistics.median(values["after"])
            summary.append(dict(probe=probe, case=name, before_us=before, after_us=after,
                                delta_pct=(after / before - 1) * 100))
    (args.out / "summary.json").write_text(json.dumps(
        dict(cpu=cpu, rounds=args.rounds, results=summary), indent=2) + "\n")
    print("probe,case,before_us,after_us,delta_pct")
    for row in summary:
        print(f"{row['probe']},{row['case']},{row['before_us']:.3f},"
              f"{row['after_us']:.3f},{row['delta_pct']:+.1f}")


if __name__ == "__main__":
    main()
