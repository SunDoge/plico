#!/usr/bin/env python3
"""Run under xvfb-run -a after cargo build -p launcher. Requires xdotool."""
import os
from pathlib import Path
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parents[3]
with tempfile.TemporaryFile(mode="w+") as log:
    app = subprocess.Popen(
        [str(root / "target/debug/launcher")], stdout=log, stderr=log,
        env={**os.environ, "WINIT_UNIX_BACKEND": "x11", "WAYLAND_DISPLAY": ""},
    )
    try:
        window = None
        for _ in range(100):
            result = subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--name", "^Plico launcher$"],
                capture_output=True, text=True,
            )
            if result.returncode == 0:
                window = result.stdout.splitlines()[0]
                break
            if app.poll() is not None:
                raise RuntimeError("launcher exited before opening")
            time.sleep(0.1)
        assert window, "window did not open"
        subprocess.run(["xdotool", "windowfocus", "--sync", window], check=True)
        time.sleep(0.5)
        subprocess.run(
            ["xdotool", "type", "--clearmodifiers", "--delay", "50", "terminal"],
            check=True,
        )
        time.sleep(0.3)
        subprocess.run(["xdotool", "key", "Escape"], check=True)
        time.sleep(0.3)
        assert app.poll() is None, "first Escape should clear the query"
        subprocess.run(["xdotool", "key", "Escape"], check=True)
        assert app.wait(timeout=8) == 0, "launcher did not close cleanly"
        print("PASS: native window, initial typing, Escape clears, Escape closes")
    except BaseException:
        log.seek(0)
        print(log.read())
        raise
    finally:
        if app.poll() is None:
            app.terminate()
            app.wait(timeout=5)
