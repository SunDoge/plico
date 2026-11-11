#!/usr/bin/env python3
"""Keep the two Widget traits on their intended sides of the boundary.

`plico::Widget` (fn show) is the immediate helper trait apps compose with;
app components are plain functions taking `&mut UiContext`. The retained
`plico_renderer::Widget` contract (measure/paint/IME/accessibility) is the
platform-widget surface implemented by framework crates such as plico-editor
and plico-terminal. Apps may *host* such a widget (import the trait to
call methods, receive `WidgetNodeEvent`), but never implement it — and never
touch the measure/paint context surface at all. This is a source-level guard,
not a Rust name resolver; aliases still require review.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Platform-widget surface apps must not reference, even to host a widget.
FORBIDDEN_NAMES = (
    "MeasureContext",
    "PaintContext",
    "WidgetGeometry",
    "WidgetHarness",
    "WidgetRasterImage",
    "WidgetAccessibility",
    "WidgetImePurpose",
    "WidgetImeState",
    "WidgetTextSelection",
    "WidgetChanges",
    "WidgetEventResult",
)
FORBIDDEN = re.compile(r"\b(" + "|".join(FORBIDDEN_NAMES) + r")\b")
IMPL_RENDERER_WIDGET = re.compile(r"impl\b[^{]*plico_renderer::(widget::)?Widget\b")

violations = []
for path in sorted((ROOT / "apps").rglob("*.rs")):
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if FORBIDDEN.search(line):
            violations.append(f"{path.relative_to(ROOT)}:{number}: platform surface: {line.strip()}")
        elif IMPL_RENDERER_WIDGET.search(line):
            violations.append(f"{path.relative_to(ROOT)}:{number}: impl: {line.strip()}")

# Interaction results are immutable snapshots queried like MyGO Elements. Keep
# structural payloads (`inner`) public, but do not expose event flags that apps
# can fabricate or mutate independently of the declaration that produced them.
response_files = {
    ROOT / "crates/plico/src/ui.rs": {
        "Response": set(),
        "ToggleResponse": set(),
        "InputResponse": set(),
    },
    ROOT / "crates/plico/src/numeric.rs": {
        "SliderResponse": set(),
        "ValueControlResponse": {"inner"},
    },
    ROOT / "crates/plico/src/modal.rs": {"ModalResponse": {"inner"}},
    ROOT / "crates/plico/src/widgets/popup.rs": {"SelectResponse": set()},
    ROOT / "crates/plico/src/widgets/search.rs": {"SearchResponse": set()},
    ROOT / "crates/plico/src/widgets/find.rs": {"FindResponse": set()},
    ROOT / "crates/plico/src/widgets/calendar.rs": {"CalendarResponse": set()},
}
for path, responses in response_files.items():
    source = path.read_text()
    for name, allowed in responses.items():
        match = re.search(
            rf"pub struct {name}(?:<[^>]+>)?\s*\{{(?P<body>.*?)^\}}",
            source,
            re.MULTILINE | re.DOTALL,
        )
        if match is None:
            violations.append(f"{path.relative_to(ROOT)}: missing response struct {name}")
            continue
        public = set(re.findall(r"^\s*pub\s+(\w+)\s*:", match.group("body"), re.MULTILINE))
        if public != allowed:
            violations.append(
                f"{path.relative_to(ROOT)}: {name} public fields {sorted(public)}; "
                f"expected {sorted(allowed)}"
            )


if violations:
    sys.exit(
        "App code must not implement the renderer Widget trait or use the retained "
        "platform-widget surface (implementations live in framework crates):\n  "
        + "\n  ".join(violations)
    )
print(f"Widget boundary holds: {sum(1 for _ in (ROOT / 'apps').rglob('*.rs'))} app sources clean")
