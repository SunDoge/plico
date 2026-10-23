#!/usr/bin/env python3
"""Check native operator typing/borrow errors against the actual Plico crate."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CASES = [
    ("nested", 'column(cx) + |cx| { row(cx) + |cx| { text(cx, "ok"); }; };', None),
    ("closure_argument", 'column(cx) + || {};', ["E0593"]),
    ("borrow_outer_context", 'column(cx) + |_| { text(cx, "wrong context"); };', ["E0500", "E0499"]),
    ("method_after_children", '(column(cx) + |cx| { text(cx, "ok"); }).padding(16);', ["E0599"]),
    ("invalid_widget_argument", 'column(cx) + |cx| { text(cx, "bad").font_size("large"); };', ["E0308"]),
]
with tempfile.TemporaryDirectory(prefix="plico-add-check-") as temporary:
    app = Path(temporary)
    (app / "src").mkdir()
    (app / "Cargo.toml").write_text(
        '[package]\nname="plico-operator-typing-probe"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\nplico-core={path=' + json.dumps(str(ROOT / "crates/plico-core")) + '}\n'
    )
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target/operator-typing-probe"))
    results = []
    for name, body, errors in CASES:
        (app / "src/main.rs").write_text(
            'use plico_core::prelude::*;\nfn screen(cx: &mut UiContext) {\n' + body + '\n}\nfn main() {}\n'
        )
        run = subprocess.run(["cargo", "check", "--offline", "--manifest-path", str(app / "Cargo.toml")],
                             cwd=ROOT, env=env, text=True, capture_output=True)
        ok = run.returncode == 0 if errors is None else run.returncode != 0 and any(f"[{code}]" in run.stderr for code in errors)
        results.append({"case": name, "passed": ok, "compiler_output": run.stderr})
    output = ROOT / "reports/macros/operator-children-compilation.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps([{k: r[k] for k in ("case", "passed")} for r in results], indent=2))
    assert all(r["passed"] for r in results), f"See {output}"
