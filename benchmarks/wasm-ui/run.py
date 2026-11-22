"""Run with xvfb-run; pass an image fixture and optionally the number of edits."""
import csv
import ctypes as c
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

here = Path(__file__).resolve().parent
repo = here.parents[1]
mode_feature = {"1": "tree", "patch": "tree-patch", "service": "reader-service,tree-patch"}.get(os.getenv("PLICO_WASM_PROBE_TREE"))
service = mode_feature is not None and "reader-service" in mode_feature
report_name = "wasm-ui-dev-loop" if service else ("wasm-ui-" + mode_feature if mode_feature else "wasm-ui")
if os.getenv("PLICO_WASM_PROBE_FAST_JIT"):
    report_name += "-fast-jit"
out = repo / "reports" / report_name
out.mkdir(parents=True, exist_ok=True)
target = Path(os.environ.get("CARGO_TARGET_DIR", repo / "target/wasm-ui-host")).resolve()
env = dict(os.environ, CARGO_TARGET_DIR=str(target))
env.pop("WAYLAND_DISPLAY", None)
env.pop("RUST_LOG", None)
image = Path(sys.argv[1]).resolve(strict=True)
rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 5
source = here / "src/ui.rs"
original = source.read_text()
module = target / "wasm32-unknown-unknown/debug/reader-ui.wasm"
live_module = out / "live.wasm"
native_bin = target / "debug/reader-host"
wasm_host = target / "debug/reader-wasm-host"

def build(*args):
    start = time.perf_counter()
    with (out / "build.log").open("w") as log:
        subprocess.run(
            ["cargo", "build", "--locked", "--offline", "-j", "8", *(["--features", mode_feature] if mode_feature else []), *args],
            cwd=here, env=env, stdout=log, stderr=log, check=True,
        )
    return 1000 * (time.perf_counter() - start)

def publish():
    pending = live_module.with_suffix(".tmp")
    shutil.copyfile(module, pending)
    pending.replace(live_module)

x11 = c.CDLL("libX11.so.6")

def signature(name, result, *arguments):
    function = getattr(x11, name)
    function.restype = result
    function.argtypes = arguments
    return function

ptr, integer, window = c.c_void_p, c.c_int, c.c_ulong
display = signature("XOpenDisplay", ptr, c.c_char_p)(None)
if not display:
    raise RuntimeError("run inside a private Xvfb display")
root = signature("XDefaultRootWindow", window, ptr)(display)
get_image = signature("XGetImage", ptr, ptr, window, integer, integer, c.c_uint, c.c_uint, window, integer)
get_pixel = signature("XGetPixel", window, ptr, integer, integer)
destroy_image = signature("XDestroyImage", integer, ptr)

def visible(process, revision):
    expected = 0xcc2233 if revision % 2 == 0 else 0x22cc55
    deadline = time.perf_counter() + 15
    while time.perf_counter() < deadline:
        if process.poll() is not None:
            raise RuntimeError("reader stopped, see window logs")
        pixel = get_image(display, root, 22, 22, 1, 1, 0xFFFFFFFF, 2)
        if not pixel:
            raise RuntimeError("could not read Xvfb pixels")
        color = get_pixel(pixel, 0, 0) & 0xFFFFFF
        destroy_image(pixel)
        if color == expected:
            return time.perf_counter()
        time.sleep(0.001)
    raise RuntimeError(f"revision {revision} was not visible")

def launch(binary, log_path, extra=(), trace=False):
    launch_env = dict(env)
    if trace:
        launch_env["RUST_LOG"] = "plico::perf=trace"
    with log_path.open("w") as log:
        return subprocess.Popen([binary, image, *extra], env=launch_env, stdout=log, stderr=log)

def stop(process):
    process.terminate()
    process.wait(timeout=10)

def memory(pid):
    fields = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines() if ":" in line)
    kb = lambda key: int(fields[key].split()[0])
    return kb("Rss"), kb("Pss")

def ready(path, revision):
    for line in reversed(path.read_text().splitlines()):
        if line.startswith(f"READY {revision} "):
            return line.split()[2:]
    raise RuntimeError(f"missing READY for {revision} in {path}")

def edit(revision):
    updated, count = re.subn(r"pub const REVISION: u32 = \d+;", f"pub const REVISION: u32 = {revision};", original)
    assert count == 1
    start = time.perf_counter()
    source.write_text(updated)
    return start

def click_during_compile(process, log, previous, offset):
    deadline = time.perf_counter() + 2.
    while process.poll() is None and time.perf_counter() < deadline:
        if "LOAD START " in log.read_text()[offset:]:
            break
        time.sleep(0.001)
    else:
        raise RuntimeError("background compilation did not start")
    expected = "false" if ready(log, previous)[6] == "true" else "true"
    started = time.perf_counter()
    subprocess.run(["xdotool", "mousemove", "225", "28", "click", "--delay", "0", "1"], env=env, check=True)
    while process.poll() is None and time.perf_counter() < deadline:
        latest = log.read_text()[offset:].splitlines()
        for line in latest:
            if line.startswith(f"READY {previous} ") and line.split()[-1] == expected:
                return 1000 * (time.perf_counter() - started)
            if line.startswith(f"READY {previous + 1} "):
                raise RuntimeError("new revision arrived before the old UI handled a click")
        time.sleep(0.001)
    raise RuntimeError("old UI did not handle the click during compilation")

with (out / "latency.csv").open("w") as results:
    writer = csv.writer(results)
    writer.writerow(["mode", "sample", "build_ms", "after_build_ms", "total_ms", "rss_kib", "pss_kib", "old_ui_click_ms"])
    process = None
    initial = 2 * (time.time_ns() % 100_000_000)
    try:
        edit(initial)
        build("--features", "wasm-host", "--bin", "reader-host")
        shutil.copyfile(native_bin, wasm_host)
        build("--bin", "reader-ui", "--target", "wasm32-unknown-unknown")
        publish()
        if not service:
            with (out / "updates.csv").open("w") as metrics:
                subprocess.run([wasm_host, "MEASURE", live_module], env=env, stdout=metrics, check=True)
        build("--bin", "reader-host")
        for mode in ("native", "wasm"):
            # Keep window startup out of the measured WASM edits, and start
            # both sequences from the same compiled revision.
            edit(initial)
            if mode == "native":
                build("--bin", "reader-host")
            else:
                build("--bin", "reader-ui", "--target", "wasm32-unknown-unknown")
                publish()
            log = out / (mode + "-window.log")
            process = launch(native_bin, log) if mode == "native" else launch(wasm_host, log, (live_module,))
            visible(process, initial)
            time.sleep(0.25)
            baseline = ready(log, initial)
            for index in range(1, rounds + 1):
                revision = initial + index
                start = edit(revision)
                build_ms = build("--bin", "reader-host") if mode == "native" else build("--bin", "reader-ui", "--target", "wasm32-unknown-unknown")
                built = time.perf_counter()
                responsiveness = None
                if mode == "native":
                    stop(process)
                    process = launch(native_bin, log)
                else:
                    offset = len(log.read_text())
                    publish()
                    if service:
                        responsiveness = round(click_during_compile(process, log, revision - 1, offset), 3)
                seen = visible(process, revision)
                state = ready(log, revision)
                assert state[:3] == baseline[:3] == ["7", "140", "17"]
                if mode == "wasm":
                    assert state[3:5] == baseline[3:5]
                    assert int(state[5]) >= int(baseline[5])
                rss, pss = memory(process.pid)
                sample = [mode, index, round(build_ms, 3), round(1000*(seen-built), 3),
                          round(1000*(seen-start), 3), rss, pss, responsiveness]
                writer.writerow(sample)
                results.flush()
                print(*sample, flush=True)
            stop(process)
            process = None
        if service:
            sys.exit(0)
        # Exercise texture lifetime independently, with tracing turned on.
        log = out / "texture-window.log"
        process = launch(wasm_host, log, (live_module,), trace=True)
        visible(process, initial + rounds)
        time.sleep(0.3)
        if mode_feature:
            subprocess.run(["xdotool", "mousemove", "264", "28", "click", "1"], env=env, check=True)
            time.sleep(0.25)
        before = ready(log, initial + rounds)
        if mode_feature:
            assert before[6] == "true", before
        before_trace = log.read_text()
        before_uploads = re.findall(r"image_uploads_total=(\d+)", before_trace)
        assert before_uploads and int(before_uploads[-1]) > 0, before_uploads
        edit(initial + rounds + 1)
        build("--bin", "reader-ui", "--target", "wasm32-unknown-unknown")
        publish()
        visible(process, initial + rounds + 1)
        time.sleep(0.3)
        after = ready(log, initial + rounds + 1)
        assert before[3:5] == after[3:5] and int(after[5]) > int(before[5])
        if mode_feature:
            assert after[6] == "true", after
        trace = log.read_text()
        after_uploads = re.findall(r"image_uploads_total=(\d+)", trace[len(before_trace):])
        assert after_uploads and all(count == before_uploads[-1] for count in after_uploads), after_uploads
        assert trace.count("RESOURCES ") == 1
        print("image uploads before and after reload:", before_uploads[-1], after_uploads[-1], flush=True)
    finally:
        source.write_text(original)
        if process and process.poll() is None:
            stop(process)
