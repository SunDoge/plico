"""Compare warm edit-to-assertion latency on the same reader component."""
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time

fixture = Path(__file__).resolve().parent
target = Path(os.environ.get("CARGO_TARGET_DIR", fixture.parents[1] / "target/wasm-ui-host")).resolve()
env = dict(os.environ, CARGO_TARGET_DIR=str(target), PLICO_WASM_PROBE_FAST_JIT="1")
for key in ("DISPLAY", "WAYLAND_DISPLAY", "PLICO_WASM_MODULE"):
    env.pop(key, None)

with tempfile.TemporaryDirectory(prefix="reader-check-", dir=fixture.parent) as scratch:
    project = Path(scratch)
    for name in ("Cargo.toml", "Cargo.lock"):
        shutil.copyfile(fixture / name, project / name)
    shutil.copytree(fixture / "src", project / "src")
    source = project / "src/tree.rs"
    original = source.read_text()
    assert original.count('"Marked"') == 1

    def build(binary, features, *args):
        subprocess.run(["cargo", "build", "--locked", "--offline", "--bin", binary,
                        "--features", features, *args], cwd=project, env=env, check=True)

    native = target / "debug/reader-host"
    module = target / "wasm32-unknown-unknown/debug/reader-ui.wasm"
    runner = project / "wasm-host"
    build("reader-host", "tree-patch,wasm-host")
    shutil.copy2(native, runner)
    modes = ["native", "native-test", "wasm"]
    samples = {mode: [] for mode in modes}
    for revision in range(6):
        offset = revision % len(modes)
        for mode in modes[offset:] + modes[:offset]:
            label = f"Marked {revision}"
            start = time.perf_counter()
            source.write_text(original.replace('"Marked"', f'"{label}"'))
            if mode == "native":
                build("reader-host", "tree-patch")
                command = [native, "CHECK", label]
            elif mode == "native-test":
                result = subprocess.run(["cargo", "test", "--locked", "--offline", "--bin", "reader-host",
                                "--features", "tree-patch", "mark_and_unmark", "--", "--exact"],
                               env=dict(env, PLICO_EXPECT_MARKED=label), cwd=project, check=True,
                               stdout=subprocess.PIPE, text=True)
                if "test mark_and_unmark ... ok" not in result.stdout:
                    raise RuntimeError(result.stdout)
                command = None
            else:
                build("reader-ui", "tree-patch", "--target", "wasm32-unknown-unknown")
                command = [runner, "CHECK", module, label]
            if command:
                subprocess.run(command, env=env, check=True)
            elapsed = time.perf_counter() - start
            if revision:
                samples[mode].append(elapsed)
                print(f"{mode} {revision}: total {elapsed:.3f}s", flush=True)
    for mode, times in samples.items():
        print(f"{mode} median {statistics.median(times):.3f}s")
