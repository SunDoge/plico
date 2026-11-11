#!/usr/bin/env python3
"""Keep application and host code on Source's stable runtime surface.

The runtime implementation may compose Engine and Adapter directly. Other
production Rust code should use Source queries/update instead of reaching
through public retained state.
Runnable examples and probes use the same stable surface. Integration tests are
migrated separately to testing::Driver and diagnostics.

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
migrated_test_roots = {
    ("apps", "backend-settings", "tests"),
    ("apps", "gallery", "tests"),
    ("apps", "launcher", "tests"),
    ("apps", "services-demo", "tests"),
    ("crates", "plico-wgpu", "tests"),
}
migrated_test_files = {
    Path("crates/plico-desktop/tests/absolute.rs"),
    Path("crates/plico-desktop/tests/backdrop.rs"),
    Path("crates/plico-desktop/tests/button_content.rs"),
    Path("crates/plico-desktop/tests/compact_flicker.rs"),
    Path("crates/plico-desktop/tests/feedback_motion.rs"),
    Path("crates/plico-desktop/tests/font_family.rs"),
    Path("crates/plico-desktop/tests/glass_pixels.rs"),
    Path("crates/plico-desktop/tests/interactive_glass.rs"),
    Path("crates/plico-desktop/tests/materials.rs"),
    Path("crates/plico-desktop/tests/native_surface.rs"),
    Path("crates/plico-desktop/tests/retained_frame.rs"),
    Path("crates/plico-desktop/tests/rich_text.rs"),
    Path("crates/plico-desktop/tests/route_nav.rs"),
    Path("crates/plico-desktop/tests/save_button_flicker.rs"),
    Path("crates/plico-desktop/tests/scene_layers.rs"),
    Path("crates/plico-desktop/tests/scroll_clip.rs"),
    Path("crates/plico-desktop/tests/text_input.rs"),
    Path("crates/plico-runtime/tests/adapter_work.rs"),
    Path("crates/plico-runtime/tests/alert_dialog.rs"),
    Path("crates/plico-runtime/tests/component_kit.rs"),
    Path("crates/plico-runtime/tests/deadline.rs"),
    Path("crates/plico-runtime/tests/display_preferences.rs"),
    Path("crates/plico-runtime/tests/focus_navigation.rs"),
    Path("crates/plico-runtime/tests/font_family.rs"),
    Path("crates/plico-runtime/tests/hover.rs"),
    Path("crates/plico-runtime/tests/indicator_semantics.rs"),
    Path("crates/plico-runtime/tests/retained_frame.rs"),
    Path("crates/plico-runtime/tests/range_slider.rs"),
    Path("crates/plico-runtime/tests/scroll_layout_motion.rs"),
    Path("crates/plico-runtime/tests/secondary_click.rs"),
    Path("crates/plico-runtime/tests/test_protocol.rs"),
    Path("crates/plico-runtime/tests/toast_motion.rs"),
    Path("crates/plico-runtime/tests/user_activity.rs"),
    Path("crates/plico-runtime/tests/variable_list.rs"),
    Path("crates/plico-runtime/tests/virtual_table_width.rs"),
}
for root in (ROOT / "apps", ROOT / "crates", ROOT / "examples"):
    for path in sorted(root.rglob("*.rs")):
        relative = path.relative_to(ROOT)
        parts = relative.parts
        migrated_test = parts[:3] in migrated_test_roots or relative in migrated_test_files
        if "tests" in parts and not migrated_test:
            continue
        if parts[:2] == ("crates", "plico-runtime"):
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
