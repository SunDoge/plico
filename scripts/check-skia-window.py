#!/usr/bin/env python3
"""Exercise Skia's real X11/Vulkan swapchain in an isolated Xvfb server."""
import os
import subprocess
import time
from pathlib import Path

from plico_smoke import Xvfb, WindowApp

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
REPORT = ROOT / "reports/skia-window"
BINARY = TARGET / "debug/examples/skia-window"

subprocess.run(
    ["cargo", "build", "--locked", "-p", "plico-vello", "--no-default-features", "--features", "skia", "--example", "skia-window"],
    cwd=ROOT, check=True,
)
REPORT.mkdir(parents=True, exist_ok=True)
with Xvfb() as server:
    env = server.env(PLICO_SKIA_DEVICE="llvmpipe", PLICO_COMPARE_VELLO=None)
    with WindowApp(BINARY, env, REPORT / "window.log") as app:
        app.window("no native window")
        app.wait(lambda: "Skia Vulkan window renderer initialized" in app.log_text, "renderer not initialized")
        app.wait_for_frame(REPORT / "initial.png")
        app.focus()
        app.click(70, 178)
        app.wait(lambda: "count=1" in app.log_text, "pointer click did not reach app")
        app.run("import", "-window", app.window(), str(REPORT / "clicked.png"))
        for width, height in [(960, 600), (700, 520), (1000, 640), (820, 480), (1000, 640)]:
            app.resize(width, height)
            time.sleep(.15)
        app.run("import", "-window", app.window(), str(REPORT / "resized.png"))
        assert app.run("identify", "-format", "%wx%h", str(REPORT / "resized.png")) == "1000x640"
        app.unmap_map()
        app.keys("Tab", "Return")
        app.wait(lambda: "count=2" in app.log_text, "keyboard activation did not reach app")
        app.keys("Tab", "Return")
        app.wait(lambda: "count=0" in app.log_text, "keyboard reset did not reach app")
        time.sleep(.2)
        app.assert_log_clean()
        app.run("xdotool", "keydown", "--window", app.window(), "Escape")
        assert app.wait_exit() == 0
        print("PASS: Skia swapchain, pointer/keyboard UI events, repeated resize, unmap/map and clean close")
