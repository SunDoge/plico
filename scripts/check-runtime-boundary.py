#!/usr/bin/env python3
"""Keep application and host code on Source's stable runtime surface.

The runtime implementation may compose Engine and Adapter directly. The paused
C# ABI is also a legacy boundary of its own. Other production Rust code should
use Source queries/update instead of reaching through public retained state.
Tests and probes are migrated separately to testing::Driver and diagnostics.

This is intentionally a source-level guard, not a Rust name resolver.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIRECT_SOURCE_FIELD = re.compile(
    r"(?:\bsource|\.source)\s*\.\s*"
    r"(engine|adapter|view|declaration_stats|declaration_micros)\b(?!\s*\()"
)

violations = []
for root in (ROOT / "apps", ROOT / "crates"):
    for path in sorted(root.rglob("*.rs")):
        relative = path.relative_to(ROOT)
        parts = relative.parts
        if "tests" in parts or "examples" in parts:
            continue
        if parts[:2] in (("crates", "plico-runtime"), ("crates", "plico-csharp")):
            continue
        for number, line in enumerate(path.read_text().splitlines(), 1):
            match = DIRECT_SOURCE_FIELD.search(line)
            if match:
                violations.append(
                    f"{relative}:{number}: Source.{match.group(1)} field: {line.strip()}"
                )

if violations:
    sys.exit(
        "Production hosts and apps must use Source's stable query/update API, "
        "not Engine/Adapter/view/stat fields directly:\n  " + "\n  ".join(violations)
    )

print("Runtime boundary holds: production hosts use Source queries/update")
