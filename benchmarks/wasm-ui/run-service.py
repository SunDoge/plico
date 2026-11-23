"""Build and exercise the same reader service in native release and WASM."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

directory = Path(__file__).resolve().parent
repo = directory.parents[1]
output = repo / "reports/wasm-ui-service"
output.mkdir(parents=True, exist_ok=True)
image = Path(sys.argv[1]).resolve(strict=True)
target = Path(os.environ.get("CARGO_TARGET_DIR", repo / "target/wasm-ui-host")).resolve()
env = dict(os.environ, CARGO_TARGET_DIR=str(target), LC_ALL="C")
env.pop("WAYLAND_DISPLAY", None)

def build(binary, features, *options):
    subprocess.run(
        ["cargo", "build", "--locked", "--offline", "-j", "8",
         "--bin", binary, "--features", features, *options],
        cwd=directory, env=env, check=True, stdout=sys.stderr,
    )

def await_value(process, description, query):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Reader stopped while waiting for {description}")
        if result := query():
            return result
        time.sleep(0.05)
    raise RuntimeError(f"Timed out waiting for {description}")

def screenshot_contains(process, expression, filename):
    path = output / filename
    def check():
        subprocess.run(
            ["import", "-window", "root", "-crop", "420x101+0+0", "+repage",
             "-resize", "200%", path], env=env, check=True,
        )
        text = subprocess.check_output(
            ["tesseract", path, "stdout", "--psm", "6"],
            stderr=subprocess.DEVNULL, text=True,
        )
        return expression in text
    await_value(process, expression, check)

build("reader-ui", "reader-service,tree-patch", "--target", "wasm32-unknown-unknown")
guest = target / "wasm32-unknown-unknown/debug/reader-ui.wasm"
build("reader-host", "reader-service,tree-patch,wasm-host")
build("reader-host", "reader-service", "--release")
runtime_deps = subprocess.check_output(
    ["cargo", "tree", "--locked", "--offline", "--features", "reader-service",
     "--edges", "normal"], text=True, cwd=directory, env=env,
)
assert "wasmtime v" not in runtime_deps
assert "rkyv v" not in runtime_deps

with tempfile.TemporaryDirectory(prefix="plico-reader-service-") as temp:
    temp = Path(temp)
    live = temp / "live.wasm"
    revised = temp / "revised.wasm"
    shutil.copyfile(guest, live)
    build("reader-ui", "reader-service-revised,tree-patch", "--target", "wasm32-unknown-unknown")
    shutil.copyfile(guest, revised)
    checksums = []
    for mode in ("native", "wasm"):
        gate = temp / f"{mode}-gate"
        log = output / f"{mode}.log"
        run_env = dict(env, PLICO_WASM_PROBE_JOB_GATE=str(gate))
        binary = target / ("release/reader-host" if mode == "native" else "debug/reader-host")
        arguments = [binary, image] + ([live] if mode == "wasm" else [])
        with log.open("w") as writer:
            process = subprocess.Popen(arguments, env=run_env, stdout=writer, stderr=writer)
        try:
            screenshot_contains(process, "Image analysis idle", f"{mode}-idle.png")
            subprocess.run(["xdotool", "mousemove", "60", "73", "click", "1"], env=env, check=True)
            screenshot_contains(process, "Analyzing image", f"{mode}-running.png")
            assert log.read_text().count("JOB START 17") == 1
            if mode == "wasm":
                assert "JOB DONE" not in log.read_text()
                revised.replace(live)
                await_value(process, "new WASM revision", lambda: "READY 1 " in log.read_text())
                screenshot_contains(process, "Analyzing image", f"{mode}-reloaded.png")
                assert "JOB DONE" not in log.read_text()
            gate.touch()
            result = await_value(process, "job completion", lambda: next(
                (line for line in log.read_text().splitlines() if line.startswith("JOB DONE 17 ")), None))
            _, _, _, width, height, checksum = result.split()
            screenshot_contains(process, f"Image {width}", f"{mode}-result.png")
            assert log.read_text().count("JOB START 17") == 1
            assert log.read_text().count("JOB DONE 17") == 1
            assert height.isdecimal()
            checksums.append(checksum)
            print(f"{mode}: image {width}x{height}, checksum {checksum}, one job", flush=True)
        finally:
            process.terminate()
            process.wait(timeout=5)
    assert checksums[0] == checksums[1]
