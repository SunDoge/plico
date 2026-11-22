"""Local server for the split demo. The browser keeps its host across guest builds."""
import json
import os
import subprocess
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

root = Path(__file__).resolve().parents[3]
target = (root / os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
site = target / "plico-web-host-split"
source = root / "apps/web-demo/src"
changed = threading.Condition()
revision = 0
error = ""
module = None


def inputs():
    return [(p, p.stat().st_mtime_ns) for p in sorted(source.rglob("*")) if p.is_file()]


def build_guest():
    result = subprocess.run([
        "cargo", "rustc", "--locked", "-p", "web-demo", "--lib", "--features", "guest",
        "--target", "wasm32-unknown-unknown", "--config", "profile.dev.package.web-demo.opt-level=0",
        "--message-format=json", "--", "-C", "strip=symbols",
    ], cwd=root, stdout=subprocess.PIPE, text=True)
    errors = []
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message["reason"] == "compiler-message":
            diagnostic = message["message"]
            rendered = diagnostic.get("rendered") or diagnostic["message"]
            print(rendered, file=sys.stderr, flush=True)
            if diagnostic["level"] == "error":
                errors.append(rendered.rstrip())
    if result.returncode:
        return None, "\n\n".join(errors) or "See the terminal for Cargo output."
    return (target / "wasm32-unknown-unknown/debug/web_demo.wasm").read_bytes(), ""


def build_loop():
    global revision, error, module
    previous = None
    while True:
        try:
            snapshot = inputs()
        except FileNotFoundError:  # editor replaced a file during the walk
            continue
        if snapshot != previous:
            previous = snapshot
            try:
                candidate, build_error = build_guest()
            except (OSError, ValueError) as exc:
                candidate, build_error = None, str(exc)
            with changed:
                if candidate is not None:
                    module = candidate
                error = build_error
                revision += 1
                changed.notify_all()
        time.sleep(0.08)


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(site), **kwargs)

    def do_GET(self):
        url = urlsplit(self.path)
        if url.path == "/" and not url.query:
            self.send_response(302)
            self.send_header("Location", "/?dev")
            self.end_headers()
            return
        if url.path == "/__changes":
            with changed:
                changed.wait_for(lambda: str(revision) != url.query, timeout=25)
                payload = json.dumps({"revision": revision, "error": error}).encode()
            mime = "application/json"
        elif url.path == "/ui-dev.wasm":
            with changed:
                payload = module
            if payload is None:
                self.send_error(503)
                return
            mime = "application/wasm"
        else:
            return super().do_GET()
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", int(os.environ.get("PORT", 8081))), Handler)
    threading.Thread(target=build_loop, daemon=True).start()
    print(f"Open http://127.0.0.1:{server.server_port}/?dev", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        server.server_close()
