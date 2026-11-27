#!/usr/bin/env python3
"""Keep application and host code on Source's stable runtime surface.

The runtime implementation may compose Engine and Adapter directly. Other
production Rust code should use Source queries/update instead of reaching
through public retained state.
Runnable examples and probes use the same stable surface. Integration tests are
migrated separately to testing::Tester and diagnostics.

Cargo's resolved dependency graph also keeps runtime tests host-independent.
"""

import re
import subprocess
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
    ("crates", "plico-desktop", "tests"),
    ("crates", "plico-runtime", "tests"),
    ("crates", "plico-wgpu", "tests"),
}
for root in (ROOT / "apps", ROOT / "crates", ROOT / "examples"):
    for path in sorted(root.rglob("*.rs")):
        relative = path.relative_to(ROOT)
        parts = relative.parts
        if "tests" in parts and parts[:3] not in migrated_test_roots:
            continue
        if parts[:3] == ("crates", "plico-runtime", "src"):
            continue
        contents = path.read_text()
        for match in DIRECT_SOURCE_FIELD.finditer(contents):
            number = contents.count("\n", 0, match.start()) + 1
            line = contents.splitlines()[number - 1].strip()
            violations.append(
                f"{relative}:{number}: Source.{match.group(1)} field: {line}"
            )

# A single application fixture used to pull the desktop and GPU stack into
# every runtime test binary. Include build and dev edges in this check.
packages = subprocess.check_output(
    ["cargo", "tree", "--locked", "-p", "plico-runtime", "-e", "normal,build,dev",
     "--prefix", "none", "--format", "{p}"],
    cwd=ROOT, text=True,
)
host_packages = {"gallery", "plico-desktop", "plico-shell", "plico-wgpu", "wgpu", "winit"}
resolved = {line.split()[0] for line in packages.splitlines() if line.strip()}
for name in sorted(host_packages & resolved):
    violations.append(f"plico-runtime tests depend on {name}; move host coverage to its owning package")

source_definition = (ROOT / "crates/plico-runtime/src/source.rs").read_text()
for field in ("engine", "adapter", "view"):
    if not re.search(rf"^\s*pub\(crate\) {field}:", source_definition, re.MULTILINE):
        violations.append(
            f"crates/plico-runtime/src/source.rs: Source.{field} must remain crate-private"
        )

runtime_root = (ROOT / "crates/plico-runtime/src/lib.rs").read_text()
if not re.search(r"^pub\(crate\) use adapter::Adapter;", runtime_root, re.MULTILINE):
    violations.append(
        "crates/plico-runtime/src/lib.rs: Adapter must remain crate-private"
    )

desktop_root = (ROOT / "crates/plico-desktop/src/lib.rs").read_text()
if re.search(r"^pub use plico_runtime::\*;", desktop_root, re.MULTILINE):
    violations.append(
        "crates/plico-desktop/src/lib.rs: desktop must explicitly curate runtime exports"
    )

dev_root = (ROOT / "crates/plico-dev/src/lib.rs").read_text()
app_trait = re.search(
    r"^pub trait App\s*\{(?P<body>.*?)^\}",
    dev_root,
    re.MULTILINE | re.DOTALL,
)
if app_trait is None:
    violations.append("crates/plico-dev/src/lib.rs: missing public App trait")
elif any(
    protocol_detail in app_trait.group("body")
    for protocol_detail in ("serde_json", "set_services", "take_service_commands")
):
    violations.append(
        "crates/plico-dev/src/lib.rs: App must not expose split-host JSON hooks"
    )
if not re.search(r"^pub\(crate\) trait ProtocolApp: App", dev_root, re.MULTILINE):
    violations.append(
        "crates/plico-dev/src/lib.rs: split-host hooks must remain on private ProtocolApp"
    )

services_root = (ROOT / "crates/plico-dev/src/services.rs").read_text()
if re.search(r"^pub trait ServiceApp\s*:\s*Default", services_root, re.MULTILINE):
    violations.append(
        "crates/plico-dev/src/services.rs: static ServiceApp must not require Default"
    )
for trait_name in ("HostService", "ServiceApp"):
    trait = re.search(
        rf"^pub trait {trait_name}\s*\{{(?P<body>.*?)^\}}",
        services_root,
        re.MULTILINE | re.DOTALL,
    )
    if trait is None:
        violations.append(
            f"crates/plico-dev/src/services.rs: missing public {trait_name} trait"
        )
    elif any(
        protocol_bound in trait.group("body")
        for protocol_bound in ("Serialize", "DeserializeOwned")
    ):
        violations.append(
            f"crates/plico-dev/src/services.rs: static {trait_name} must not require serialization"
        )

if violations:
    sys.exit(
        "Runtime/API boundary failed:\n  " + "\n  ".join(violations)
    )

print("Runtime boundary holds: stable Source and application lifecycle APIs")
