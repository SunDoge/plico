#!/usr/bin/env python3
"""Drive a generated RegionApp through real window input and automatic dylib reload."""
import os
import re
import signal
import subprocess
import tempfile
from pathlib import Path

from plico_smoke import WindowApp, Xvfb

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
CLI = TARGET / "debug" / ("plico.exe" if os.name == "nt" else "plico")
REPORT = ROOT / "reports/regions-window-reload"
REPORT.mkdir(parents=True, exist_ok=True)


def crop_hash(app, image, geometry):
    return app.run("convert", str(image), "-crop", geometry, "+repage", "-format", "%#", "info:")


with tempfile.TemporaryDirectory(prefix="plico regions window reload ") as temp:
    directory = Path(temp) / "app"
    subprocess.run([str(CLI), "new", str(directory), "--name", "region-reload-window"], check=True)
    source = directory / "src/lib.rs"
    original = source.read_text()
    with Xvfb() as server:
        env = server.env(PLICO_SKIA_DEVICE="llvmpipe")
        with WindowApp(CLI, env, REPORT / "window.log", args=("dev", str(directory))) as app:
            window = None
            host_pid = None
            try:
                def find_window():
                    result = subprocess.run(
                        ["xdotool", "search", "--onlyvisible", "--name", "^Plico gallery$"],
                        env=env, capture_output=True, text=True,
                    )
                    found = result.stdout.strip()
                    if found:
                        app._window_id = found.splitlines()[0]
                    return bool(found)

                app.wait(find_window, "generated RegionApp window did not open", timeout=300)
                window = app.window()
                app.wait_for_frame(REPORT / "initial.png")
                match = re.search(r"resident host PID (\d+)", app.log_text)
                assert match, app.log_text
                host_pid = int(match.group(1))
                app.focus()
                initial_counter = crop_hash(app, REPORT / "initial.png", "220x30+20+65")
                app.click(160, 120)
                clicked = app.capture(REPORT / "clicked.png")
                clicked_counter = crop_hash(app, clicked, "220x30+20+65")
                assert clicked_counter != initial_counter, "native click did not change the counter"

                source.write_text(original.replace("Hello Plico", "Reloaded Window"))
                app.wait(
                    lambda: "activated application revision" in app.log_text,
                    "watcher did not activate the rebuilt dylib",
                    timeout=180,
                )
                reloaded = app.capture(REPORT / "reloaded.png")
                assert app.window() == window, "reload replaced the native window"
                assert crop_hash(app, reloaded, "220x30+20+65") == clicked_counter, (
                    "compatible reload did not preserve the visible counter"
                )
                assert crop_hash(app, reloaded, "360x45+20+20") != crop_hash(
                    app, clicked, "360x45+20+20"
                ), "title pixels did not change after reload"
                app.assert_log_clean()
                print(
                    "PASS: generated RegionApp native window, pointer input, compiler watcher, "
                    "automatic dylib activation and compatible visible state"
                )
            finally:
                if host_pid is not None:
                    try:
                        os.kill(host_pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
