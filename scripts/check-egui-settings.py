#!/usr/bin/env python3
"""Exercise the shared backend settings UI on the egui/epaint backend in an isolated Xvfb server."""
import subprocess
from pathlib import Path

from plico_smoke import Xvfb, WindowApp

ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "reports/egui-settings"

subprocess.run(
    [
        "cargo",
        "build",
        "--locked",
        "-p",
        "backend-settings",
        "--features",
        "egui",
        "--bin",
        "backend-settings",
    ],
    cwd=ROOT,
    check=True,
)
REPORT.mkdir(parents=True, exist_ok=True)
with Xvfb() as server:
    env = server.env(PLICO_PERFORMANCE_HUD="1")
    with WindowApp(
        ROOT / "target/debug/backend-settings",
        env,
        REPORT / "window.log",
        args=["--backend", "egui"],
    ) as app:
        app.window("no settings window")
        app.focus()

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
        app.resize(520, 760)
        capture("narrow")
        assert (
            app.run("identify", "-format", "%wx%h", str(REPORT / "narrow.png")) == "520x760"
        )
        print(
            "PASS: egui settings routes, Details, theme and narrow resize; captures in reports/egui-settings"
        )
