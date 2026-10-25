"""Run after cargo build -p terminal, under xvfb-run -a."""
import ctypes
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]

def wait_for(check, seconds=20):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError("native terminal smoke timeout")

def xdo(*args):
    return subprocess.run(["xdotool", *map(str, args)], check=True, capture_output=True, text=True).stdout.strip()

def request_close(window):
    # Send WM_DELETE_WINDOW rather than destroying the X window underneath
    # winit, so the host has a chance to run normal teardown.
    x = ctypes.CDLL("libX11.so.6")
    x.XOpenDisplay.restype = ctypes.c_void_p
    display = x.XOpenDisplay(None)
    assert display
    x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x.XInternAtom.restype = ctypes.c_ulong
    class Data(ctypes.Union):
        _fields_ = [("l", ctypes.c_long * 5)]
    class Client(ctypes.Structure):
        _fields_ = [("type",ctypes.c_int),("serial",ctypes.c_ulong),
            ("send_event",ctypes.c_int),("display",ctypes.c_void_p),
            ("window",ctypes.c_ulong),("message_type",ctypes.c_ulong),
            ("format",ctypes.c_int),("data",Data)]
    class Event(ctypes.Union):
        _fields_ = [("client",Client),("pad",ctypes.c_long * 24)]
    event=Event()
    event.client=Client(33,0,1,display,int(window),
        x.XInternAtom(display,b"WM_PROTOCOLS",0),32,Data())
    event.client.data.l[0]=x.XInternAtom(display,b"WM_DELETE_WINDOW",0)
    x.XSendEvent.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.c_int,ctypes.c_long,ctypes.POINTER(Event)]
    x.XSendEvent(display,int(window),0,0,ctypes.byref(event))
    x.XFlush.argtypes=[ctypes.c_void_p]
    x.XFlush(display)
    x.XCloseDisplay.argtypes=[ctypes.c_void_p]
    x.XCloseDisplay(display)

with tempfile.TemporaryDirectory(prefix="terminal-") as directory:
    folder = Path(directory)
    before, after, received = [folder / n for n in ("before", "after", "received")]
    command = (
        "printf '\033[1;34mPlico terminal - PTY ready\033[0m\r\n'; "
        f"stty size > {shlex.quote(str(before))}; "
        "IFS= read -r line; "
        f'printf "%s" "$line" > {shlex.quote(str(received))}; '
        'printf "\r\nRECEIVED: %s\r\n" "$line"; '
        "IFS= read -r second; "
        f"stty size > {shlex.quote(str(after))}; "
        'printf "\r\nResize observed. Process exited cleanly.\r\n"'
    )
    env = dict(os.environ, WINIT_UNIX_BACKEND="x11", WAYLAND_DISPLAY="")
    with (folder / "app.log").open("w") as log:
        process = subprocess.Popen(
            [str(ROOT / "target/debug/terminal"), "--command", "/bin/sh",
             "--command-args", json.dumps(["-c", command])],
            cwd=ROOT, env=env, stdout=log, stderr=log,
        )
        try:
            def window():
                result = subprocess.run(
                    ["xdotool", "search", "--name", "^Plico Terminal$"],
                    capture_output=True, text=True,
                )
                return result.stdout.strip().splitlines()[0] if result.stdout.strip() else None
            win = wait_for(window)
            wait_for(lambda: before.exists() and before.read_text().strip())
            xdo("windowfocus", "--sync", win)
            xdo("mousemove", "--window", win, 180, 170)
            xdo("click", 1)
            xdo("type", "--clearmodifiers", "--delay", 20, "terminal-smoke")
            xdo("key", "Return")
            wait_for(lambda: received.exists() and received.read_text() == "terminal-smoke")
            # Repeated Plico UI declarations must not replace the PTY.
            xdo("mousemove", "--window", win, 900, 30)
            xdo("click", 1)
            xdo("windowsize", win, 800, 520)
            time.sleep(0.2)
            xdo("mousemove", "--window", win, 180, 170)
            xdo("click", 1)
            xdo("type", "--clearmodifiers", "--delay", 20, "resize-check")
            xdo("key", "Return")
            wait_for(lambda: after.exists() and after.read_text().strip())
            initial = list(map(int, before.read_text().split()))
            final = list(map(int, after.read_text().split()))
            assert initial != final, (initial, final)
            assert all(value > 0 for value in final)
            capture = os.environ.get("PLICO_TERMINAL_CAPTURE")
            if capture:
                time.sleep(0.2)
                subprocess.run(["import", "-window", win, capture], check=True)
            print(json.dumps({"input":received.read_text(), "grid_before":initial, "grid_after":final}))
            request_close(win)
            process.wait(timeout=10)
            assert process.returncode == 0, process.returncode
        except BaseException:
            print((folder / "app.log").read_text(), flush=True)
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
            if process.returncode not in (0, -15):
                print((folder / "app.log").read_text())
