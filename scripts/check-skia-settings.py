#!/usr/bin/env python3
"""Exercise the shared backend settings UI on Skia's X11/Vulkan swapchain in an isolated Xvfb server."""
import os
import subprocess
from pathlib import Path

from plico_smoke import Xvfb, WindowApp

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
REPORT = ROOT / "reports/skia-settings"
BINARY = TARGET / "debug/settings-skia"

subprocess.run(
    ["cargo", "build", "--locked", "-p", "plico-backend-settings", "--features", "skia", "--bin", "settings-skia"],
    cwd=ROOT, check=True,
)
REPORT.mkdir(parents=True, exist_ok=True)
with Xvfb() as server:
    env = server.env(PLICO_SKIA_DEVICE="llvmpipe", PLICO_COMPARE_VELLO=None, PLICO_PERFORMANCE_HUD="1")
    with WindowApp(BINARY, env, REPORT / "window.log") as app:
        app.window("no settings window")
        app.wait(lambda: "Skia Vulkan window renderer initialized" in app.log_text, "renderer not initialized")
        app.focus()
        app.wait_for_frame(REPORT / "ready.png")

        def capture(name):
            return app.capture_hash(REPORT / (name + ".png"))

        initial = capture("overview")
        app.click(1030, 97)
        assert capture("details") != initial, "Details did not change the view"
        app.click(80, 142)
        activity = capture("activity")
        assert activity != initial, "Activity navigation did not change the view"
        app.click(80, 182)
        preferences = capture("preferences")
        assert preferences != activity, "Preferences navigation did not change the view"
        app.click(1000, 34)
        assert capture("dark") != preferences, "Theme did not change the view"
        # The performance HUD panel (light fill + blurred box shadow) must
        # stay painted in the dark theme; a failed shadow frame blanks the window.
        hud = [int(v) for v in app.run(
            "identify", "-format", "%[pixel:p{1050,60}]", str(REPORT / "dark.png")
        ).strip("srgb(").rstrip(")").split(",")]
        assert all(v > 180 for v in hud[:3]), f"HUD panel not painted: {hud}"
        app.resize(520, 760)
        capture("narrow")
        assert app.run("identify", "-format", "%wx%h", str(REPORT / "narrow.png")) == "520x760"
        print("PASS: Skia settings routes, Details, theme and narrow resize; captures in reports/skia-settings")
