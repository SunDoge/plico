#!/usr/bin/env python3
"""Classify changed CI inputs; unknown paths always retain the full build."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
QUALITY_ONLY = {".github/dependabot.yml", "release-plz.toml", ".gitignore", "LICENSE", "NOTICE"}


def requires_build(paths):
    return any(not (path.endswith(".md") or path in QUALITY_ONLY) for path in paths)


def main():
    event_name = os.environ["GITHUB_EVENT_NAME"]
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    # Manual runs always provide the full verification path.
    build = True
    if event_name in ("push", "pull_request"):
        base = event["pull_request"]["base"]["sha"] if event_name == "pull_request" else event.get("before")
        head = event["pull_request"]["head"]["sha"] if event_name == "pull_request" else event.get("after")
        if base and head and set(base) != {"0"}:
            try:
                # Fetch exact commits rather than relying on shallow checkout ancestry.
                for sha in (base, head):
                    if subprocess.run(["git", "cat-file", "-e", sha], cwd=ROOT, capture_output=True).returncode:
                        subprocess.run(["git", "fetch", "--no-tags", "--depth=1", "origin", sha], cwd=ROOT, check=True)
                paths = subprocess.check_output(["git", "diff", "--no-renames", "--name-only", "-z", base, head], cwd=ROOT)
                changed = [p.decode("utf-8", errors="surrogateescape") for p in paths.split(b"\0") if p]
                build = requires_build(changed)
                print("Changed inputs:", *changed, sep="\n  ")
            except (OSError, subprocess.CalledProcessError) as error:
                # Missing history must never silently remove build coverage.
                print(f"Could not classify changes; running all checks: {error}")
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"build={'true' if build else 'false'}\n")
    print(f"Full build required: {build}")


if __name__ == "__main__":
    main()
