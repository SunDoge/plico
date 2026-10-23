#!/usr/bin/env python3
"""SDL mouse/keyboard/close smoke test. Run inside xvfb-run -a on Linux."""
import ctypes as c
import os
from pathlib import Path
import subprocess
import tempfile
import time


def close_window(window):
    # Send the same WM_DELETE_WINDOW client message as a window manager.
    # XDestroyWindow would bypass the normal SDL close-event path.
    x = c.CDLL("libX11.so.6")
    x.XOpenDisplay.restype = c.c_void_p
    x.XOpenDisplay.argtypes = [c.c_char_p]
    x.XInternAtom.restype = c.c_ulong
    x.XInternAtom.argtypes = [c.c_void_p, c.c_char_p, c.c_int]
    x.XSendEvent.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_long, c.c_void_p]
    x.XFlush.argtypes = [c.c_void_p]
    x.XCloseDisplay.argtypes = [c.c_void_p]

    class ClientMessage(c.Structure):
        _fields_ = [("type", c.c_int), ("serial", c.c_ulong), ("send_event", c.c_int),
                    ("display", c.c_void_p), ("window", c.c_ulong),
                    ("message_type", c.c_ulong), ("format", c.c_int), ("data", c.c_long * 5)]

    class Event(c.Union):
        _fields_ = [("client", ClientMessage), ("padding", c.c_long * 24)]

    display = x.XOpenDisplay(None)
    if not display:
        raise RuntimeError("cannot open X11 display; run with xvfb-run -a")
    try:
        event = Event()
        event.client.type = 33  # ClientMessage
        event.client.window = window
        event.client.message_type = x.XInternAtom(display, b"WM_PROTOCOLS", 0)
        event.client.format = 32
        event.client.data[0] = x.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
        if not x.XSendEvent(display, window, 0, 0, c.byref(event)):
            raise RuntimeError("could not send window close request")
        x.XFlush(display)
    finally:
        x.XCloseDisplay(display)


def main():
    root = Path(__file__).resolve().parents[1]
    target = Path(os.environ.get("CARGO_TARGET_DIR", root / "target"))
    binary = target / "debug/examples/counter"
    env = dict(os.environ, SDL_VIDEODRIVER="x11", SDL_RENDER_DRIVER="software")
    with tempfile.TemporaryDirectory(prefix="plico-sdl-") as directory:
        screenshot = Path(directory) / "counter.png"
        with tempfile.TemporaryFile(mode="w+") as output:
            process = subprocess.Popen([str(binary), "--out", str(screenshot)], env=env,
                                       stdout=output, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 10
                while True:
                    result = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Plico / LVGL$"],
                                            capture_output=True, text=True)
                    if result.returncode == 0:
                        window = result.stdout.splitlines()[0]
                        break
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("SDL window did not open")
                    time.sleep(0.05)
                # This fixed-size fixture places Increment at (24, 97)-(138, 127).
                time.sleep(0.15)
                subprocess.run(["xdotool", "mousemove", "--window", window, "80", "112", "click", "1"], check=True)
                time.sleep(0.15)
                subprocess.run(["xdotool", "windowfocus", "--sync", window, "key", "Return"], check=True)
                time.sleep(0.15)
                close_window(int(window))
                code = process.wait(timeout=5)
                output.seek(0)
                log = output.read()
                assert code == 0, f"exit={code}\n{log}"
                assert "window closed: count=2" in log, log
                assert screenshot.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
                print("PASS: SDL window, mouse click, keyboard activation, deferred close and snapshot")
            except BaseException:
                output.seek(0)
                print(output.read())
                raise
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)


if __name__ == "__main__":
    main()
