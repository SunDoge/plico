#!/usr/bin/env python3
"""Check text history/outline invariants on the production offscreen GPU path.

Requires installed Noto Sans and Noto Sans CJK SC. Output is disposable.
This does not test the desktop compositor or classify punctuation as noise.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("reports/text-artifacts"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = (root / args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "--locked", "--offline", "-p", "plico-shell",
                    "--example", "text_quality_probe"], cwd=root, check=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version=1", "--no-deps", "--offline"], cwd=root))
    binary = Path(metadata["target_directory"]) / "debug/examples/text_quality_probe"
    env = os.environ.copy()
    env.pop("PLICO_TEXT_STEM_BOOST", None)
    results = {}
    for name, family, text in [
        ("latin", "Noto Sans", "Hi ill .,:; 0123 MW / Save settings"),
        ("cjk", "Noto Sans CJK SC", "口目日田中 设置连接保存 ：，。密集笔画"),
        ("mixed", None, "中文字体渲染：设置 / Hello Rust .,:; 0123"),
    ]:
        for theme, background, foreground in [
            ("light", "#ffffff", "#18181b"), ("dark", "#18181b", "#f4f4f5")]:
            directory = output / f"{name}-{theme}"
            directory.mkdir(exist_ok=True)
            rows = [dict(text=text, size=size, weight=weight,
                         x=20 + index * .125, baseline=32 + index * 40 + index * .125)
                    for index, (weight, size) in enumerate(
                        (w, s) for w in (400, 500) for s in (12, 14, 16))]
            case = dict(width=660, height=260, background=background,
                        foreground=foreground, fontFamily=family,
                        scales=[1, 1.25, 1.5, 1.75, 2], rows=rows)
            manifest = directory / "case.json"
            manifest.write_text(json.dumps(case, ensure_ascii=False, indent=2) + "\n")
            subprocess.run([str(binary), "--cases", str(manifest), "--output",
                            str(directory), "--check-artifacts"], cwd=root, env=env, check=True)
            results[f"{name}-{theme}"] = json.loads((directory / "artifacts.json").read_text())
    (output / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"Passed {sum(len(r['cases']) for r in results.values())} fixture/scale combinations: {output}")


if __name__ == "__main__":
    main()
