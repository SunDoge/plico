#!/usr/bin/env python3
"""Exercise Skia's real X11/Vulkan swapchain in an isolated Xvfb server."""
from pathlib import Path
import os
import selectors
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
REPORT = ROOT / "reports/skia-window"
REPORT.mkdir(parents=True, exist_ok=True)
BINARY = TARGET / "debug/examples/skia-window"
subprocess.run(["cargo", "build", "--locked", "-p", "plico-vello", "--features", "skia", "--example", "skia-window"], cwd=ROOT, check=True)
server = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-dpi", "96", "-nolisten", "tcp"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
try:
    selector = selectors.DefaultSelector()
    selector.register(server.stdout, selectors.EVENT_READ)
    if not selector.select(10):
        raise RuntimeError("Xvfb did not report its display")
    display = server.stdout.readline().strip()
    if not display.isdecimal():
        raise RuntimeError("Xvfb startup failed: " + server.stderr.read())
    env = {**os.environ, "DISPLAY": ":" + display, "WINIT_UNIX_BACKEND": "x11", "WINIT_X11_SCALE_FACTOR": "1", "PLICO_SKIA_DEVICE": "llvmpipe"}
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("PLICO_COMPARE_VELLO", None)
    with (REPORT / "window.log").open("w") as log:
        app = subprocess.Popen([str(BINARY)], cwd=ROOT, env=env, stdout=log, stderr=log)
        def command(*args):
            return subprocess.check_output(args, env=env, text=True, timeout=15).strip()
        def wait(predicate, message, timeout=20):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if predicate(): return
                if app.poll() is not None:
                    raise RuntimeError(f"app exited {app.returncode}: " + (REPORT / "window.log").read_text())
                time.sleep(.05)
            raise RuntimeError(message + ": " + (REPORT / "window.log").read_text())
        def find_window():
            result = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
            return result.stdout.strip().splitlines() if result.returncode == 0 else []
        try:
            wait(lambda: bool(find_window()), "no native window")
            window = find_window()[0]
            wait(lambda: "Skia Vulkan window renderer initialized" in (REPORT / "window.log").read_text(), "renderer not initialized")
            time.sleep(.5)
            command("import", "-window", window, str(REPORT / "initial.png"))
            command("xdotool", "windowfocus", "--sync", window)
            command("xdotool", "mousemove", "--window", window, "70", "178", "click", "1")
            wait(lambda: "count=1" in (REPORT / "window.log").read_text(), "pointer click did not reach app")
            command("import", "-window", window, str(REPORT / "clicked.png"))
            for width, height in [(960, 600), (700, 520), (1000, 640), (820, 480), (1000, 640)]:
                command("xdotool", "windowsize", window, str(width), str(height))
                time.sleep(.15)
            command("import", "-window", window, str(REPORT / "resized.png"))
            assert command("identify", "-format", "%wx%h", str(REPORT / "resized.png")) == "1000x640"
            command("xdotool", "windowunmap", window)
            time.sleep(.1)
            command("xdotool", "windowmap", window)
            command("xdotool", "windowfocus", "--sync", window)
            command("xdotool", "key", "--window", window, "Tab", "Return")
            wait(lambda: "count=2" in (REPORT / "window.log").read_text(), "keyboard activation did not reach app")
            command("xdotool", "key", "--window", window, "Tab", "Return")
            wait(lambda: "count=0" in (REPORT / "window.log").read_text(), "keyboard reset did not reach app")
            time.sleep(.2)
            contents = (REPORT / "window.log").read_text()
            assert "window presentation failed" not in contents, contents
            assert "panicked" not in contents, contents
            assert "Validation Error" not in contents, contents
            assert "SYNC-HAZARD" not in contents, contents
            command("xdotool", "keydown", "--window", window, "Escape")
            assert app.wait(timeout=10) == 0
            print("PASS: Skia swapchain, pointer/keyboard UI events, repeated resize, unmap/map and clean close")
        finally:
            if app.poll() is None:
                app.terminate()
                try: app.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    app.kill(); app.wait()
finally:
    server.terminate()
    try: server.wait(timeout=5)
    except subprocess.TimeoutExpired:
        server.kill(); server.wait()
