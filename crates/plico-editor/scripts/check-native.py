#!/usr/bin/env python3
"""Isolated X11 editor smoke; build the gallery code_editor example first."""
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
with tempfile.TemporaryDirectory(prefix="plico-editor-x11-") as temporary:
    directory = Path(temporary)
    with (directory / "xvfb.log").open("w") as log:
        xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1100x800x24"], stdout=subprocess.PIPE, stderr=log, text=True)
        display = xvfb.stdout.readline().strip()
        env = dict(os.environ, DISPLAY=f":{display}", WAYLAND_DISPLAY="", WINIT_UNIX_BACKEND="x11")
        app = None
        try:
            with (directory / "app.log").open("w") as app_log:
                app = subprocess.Popen([str(ROOT / "target/debug/examples/code_editor")], cwd=ROOT, env=env, stdout=app_log, stderr=app_log)
                def command(*args, **kwargs):
                    return subprocess.check_output(args, env=env, **kwargs)
                window = None
                for _ in range(600):
                    if app.poll() is not None:
                        raise RuntimeError((directory / "app.log").read_text())
                    result = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Code editor"], env=env, capture_output=True, text=True)
                    if result.returncode == 0:
                        window = result.stdout.splitlines()[0]
                        break
                    time.sleep(.1)
                assert window, "editor window did not appear: " + (directory / "app.log").read_text()
                command("xdotool", "windowfocus", "--sync", window)
                time.sleep(1)
                command("xdotool", "mousemove", "--window", window, "250", "180", "click", "1")
                sample = 'fn main() {\n    println!("native 中文 clipboard");\n}\n'
                subprocess.run(["xclip", "-selection", "clipboard"], input=sample.encode(), env=env, check=True)
                command("xdotool", "key", "ctrl+a", "ctrl+v")
                time.sleep(.4)
                command("xdotool", "key", "ctrl+a", "ctrl+c")
                time.sleep(.3)
                assert command("xclip", "-selection", "clipboard", "-o").decode() == sample
                # Undo the paste, redo it, and check the document through real copy.
                command("xdotool", "key", "ctrl+z", "ctrl+shift+z", "ctrl+a", "ctrl+c")
                time.sleep(.3)
                assert command("xclip", "-selection", "clipboard", "-o").decode() == sample
                command("xdotool", "key", "Right", "End", "Return")
                command("xdotool", "type", "--clearmodifiers", "--delay", "25", "// native input verified")
                command("xdotool", "key", "ctrl+a", "ctrl+c")
                time.sleep(.3)
                copied = command("xclip", "-selection", "clipboard", "-o").decode()
                assert "// native input verified" in copied, repr(copied)
                command("xdotool", "key", "Right")
                time.sleep(.2)
                screenshot = os.environ.get("PLICO_EDITOR_SCREENSHOT", "/tmp/plico-editor-native.png")
                command("import", "-window", window, screenshot)
                print("PASS: native focus, UTF-8 paste/copy, undo/redo, multiline typing")
                print(f"Screenshot: {screenshot}")
        finally:
            if app is not None:
                app.terminate()
                app.wait(timeout=10)
            xvfb.terminate()
            xvfb.wait(timeout=10)
