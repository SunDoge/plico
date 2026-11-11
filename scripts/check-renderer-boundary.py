#!/usr/bin/env python3
"""Keep the shared renderer contract independent from concrete GPU APIs."""

from pathlib import Path
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "crates/plico-renderer/Cargo.toml"
SOURCE = ROOT / "crates/plico-renderer/src"


def main() -> int:
    manifest = tomllib.loads(MANIFEST.read_text())
    dependency_tables = [manifest.get("dependencies", {})]
    dependency_tables.extend(
        target.get("dependencies", {})
        for target in manifest.get("target", {}).values()
    )
    if any("wgpu" in dependencies for dependencies in dependency_tables):
        print("plico-renderer must not depend on wgpu; put GPU execution in plico-wgpu")
        return 1

    violations = []
    for path in SOURCE.rglob("*.rs"):
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if "wgpu::" in line:
                violations.append(f"{path.relative_to(ROOT)}:{number}: {line.strip()}")
    if violations:
        print("plico-renderer contains concrete wgpu API usage:")
        print("\n".join(violations))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
