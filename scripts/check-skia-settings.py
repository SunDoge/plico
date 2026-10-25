#!/usr/bin/env python3
"""Exercise the shared backend settings UI on Skia's X11/Vulkan swapchain in an isolated Xvfb server."""
from pathlib import Path
import os
import selectors
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
REPORT = ROOT / "reports/skia-settings"
REPORT.mkdir(parents=True, exist_ok=True)
BINARY = TARGET / "debug/settings-skia"
subprocess.run(["cargo", "build", "--locked", "-p", "plico-backend-settings", "--features", "skia", "--bin", "settings-skia"], cwd=ROOT, check=True)
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
            wait(lambda: bool(find_window()), "no settings window")
            window = find_window()[0]
            wait(lambda: "Skia Vulkan window renderer initialized" in (REPORT / "window.log").read_text(), "renderer not initialized")
            command("xdotool", "windowfocus", "--sync", window)
            def capture(name):
                time.sleep(.4)
                command("import", "-window", window, str(REPORT / (name + ".png")))
                errors = (REPORT / "window.log").read_text()
                assert not any(s in errors for s in ("window presentation failed", "panicked", "Validation Error", "SYNC-HAZARD")), errors
                return command("identify", "-format", "%#", str(REPORT / (name + ".png")))
            def click(x,y):
                command("xdotool", "mousemove", "--window", window, str(x), str(y), "click", "1")
            initial = capture("overview")
            click(1030,97)
            assert capture("details") != initial, "Details did not change the view"
            click(80,142)
            activity = capture("activity")
            assert activity != initial, "Activity navigation did not change the view"
            click(80,182)
            preferences = capture("preferences")
            assert preferences != activity, "Preferences navigation did not change the view"
            click(1000,34)
            assert capture("dark") != preferences, "Theme did not change the view"
            command("xdotool", "windowsize", window, "520", "760")
            capture("narrow")
            assert command("identify", "-format", "%wx%h", str(REPORT / "narrow.png")) == "520x760"
            print("PASS: Skia settings routes, Details, theme and narrow resize; captures in reports/skia-settings")
        finally:
            if app.poll() is None:
                app.terminate()
                app.wait(timeout=10)
finally:
    server.terminate()
    server.wait(timeout=10)
