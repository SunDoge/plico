"""Shared harness for native window smoke tests: Xvfb + xdotool + ImageMagick.

Importable from sibling scripts (`python3 scripts/check-*.py` puts this
directory on sys.path). It owns only mechanics that every window check needs:
an isolated Xvfb display, launching an app with a log file, finding its window,
sending input, capturing screenshots and scanning the log for failures.
Test-specific assertions stay in the calling scripts.
"""
import os
import selectors
import socket
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOG_FAILURES = ("window presentation failed", "panicked", "Validation Error", "SYNC-HAZARD")


class Xvfb:
    """An isolated Xvfb display; use as a context manager."""

    def __init__(self, screen="1280x900x24"):
        self.screen = screen
        self.display = None
        self._server = None

    def __enter__(self):
        self._server = subprocess.Popen(
            ["Xvfb", "-displayfd", "1", "-screen", "0", self.screen, "-dpi", "96", "-nolisten", "tcp"],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        selector = selectors.DefaultSelector()
        selector.register(self._server.stdout, selectors.EVENT_READ)
        try:
            if not selector.select(10):
                raise RuntimeError("Xvfb did not report its display")
            display = self._server.stdout.readline().strip()
            if not display.isdecimal():
                raise RuntimeError("Xvfb startup failed: " + self._server.stderr.read())
            self.display = display
            # displayfd only allocates the number; the socket may not listen yet.
            # Wait for it to accept connections so first apps cannot race Xvfb.
            deadline = time.monotonic() + 5
            while True:
                try:
                    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as probe:
                        probe.connect(f"/tmp/.X11-unix/X{self.display}")
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"Xvfb socket :{self.display} never listened")
                    time.sleep(.02)
        except BaseException:
            self._stop()
            raise
        return self

    def env(self, **extra):
        env = {
            **os.environ,
            "DISPLAY": ":" + self.display,
            "WINIT_UNIX_BACKEND": "x11",
            "WINIT_X11_SCALE_FACTOR": "1",
        }
        env.pop("WAYLAND_DISPLAY", None)
        env.update({key: value for key, value in extra.items() if value is not None})
        for key, value in extra.items():
            if value is None:
                env.pop(key, None)
        return env

    def _stop(self):
        if self._server is not None:
            self._server.terminate()
            try:
                self._server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self._server.kill()
                self._server.wait(timeout=10)
            self._server = None

    def __exit__(self, *exc):
        self._stop()
        return False


class WindowApp:
    """One native app process under test on an existing Xvfb display."""

    def __init__(self, binary, env, log, cwd=ROOT, settle=0.05, args=()):
        self.binary = str(binary)
        self.args = tuple(args)
        self.env = env
        self.log = Path(log)
        self.cwd = cwd
        self.settle = settle
        self.process = None
        self._window_id = None
        self.log.parent.mkdir(parents=True, exist_ok=True)

    def __enter__(self):
        self.log.write_text("")
        self.process = subprocess.Popen(
            [self.binary, *self.args], cwd=self.cwd, env=self.env,
            stdout=self.log.open("w"), stderr=subprocess.STDOUT,
        )
        return self

    def __exit__(self, *exc):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
        return False

    @property
    def log_text(self):
        return self.log.read_text()

    def run(self, *args, timeout=15):
        return subprocess.check_output(
            [str(a) for a in args], env=self.env, text=True, timeout=timeout
        ).strip()

    def wait(self, predicate, message, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            if self.process.poll() is not None:
                raise RuntimeError(f"app exited {self.process.returncode}: {self.log_text}")
            time.sleep(self.settle)
        raise RuntimeError(f"{message}: {self.log_text}")

    def window_ids(self):
        result = subprocess.run(
            ["xdotool", "search", "--onlyvisible", "--pid", str(self.process.pid)],
            env=self.env, capture_output=True, text=True,
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip().splitlines()
        return []

    def window(self, message="no native window"):
        # A mapped window keeps its XID while hidden. Searching only visible
        # windows again would make unmap/map wait forever for its own window.
        if self._window_id is None:
            found = []
            def discover():
                found[:] = self.window_ids()
                return bool(found)
            self.wait(discover, message)
            self._window_id = found[0]
        return self._window_id

    def focus(self):
        self.run("xdotool", "windowfocus", "--sync", self.window())

    def click(self, x, y):
        self.run("xdotool", "mousemove", "--window", self.window(), str(x), str(y), "click", "1")

    def keys(self, *keys):
        self.run("xdotool", "key", "--window", self.window(), *keys)

    def resize(self, width, height):
        self.run("xdotool", "windowsize", self.window(), str(width), str(height))

    def unmap_map(self):
        self.run("xdotool", "windowunmap", self.window())
        time.sleep(.1)
        self.run("xdotool", "windowmap", self.window())
        self.focus()

    def capture(self, path, settle=0.4):
        """Screenshot to `path`, refusing to keep a frame whose log shows failures."""
        time.sleep(settle)
        self.run("import", "-window", self.window(), str(path))
        self.assert_log_clean()
        return path

    def wait_for_frame(self, path, timeout=20):
        """Wait for real UI pixels rather than assuming pipeline startup took <400ms.

        The smoke apps have nonuniform content in their first frame. X11 maps
        their windows before the first GPU submission, initially exposing black.
        """
        def ready():
            self.capture(path, settle=0)
            return int(self.run("identify", "-format", "%k", str(path))) > 1
        self.wait(ready, "window never presented its first nonuniform frame", timeout=timeout)

    def capture_hash(self, path, settle=0.4):
        self.capture(path, settle)
        return self.run("identify", "-format", "%#", str(path))

    def assert_log_clean(self):
        errors = self.log_text
        assert not any(s in errors for s in LOG_FAILURES), errors

    def wait_exit(self, timeout=10):
        return self.process.wait(timeout=timeout)
