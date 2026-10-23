#!/usr/bin/env python3
"""Real cdylib + retained host: scrolling and activation retain the row window."""
import json
import os
from pathlib import Path
import shutil
import queue
import threading
import time
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
subprocess.run(["cargo", "build", "-p", "plico-gallery", "-p", "plico-backend", "--lib", "--bin", "plico-backend", "--quiet"], cwd=ROOT, check=True)

def texts(nodes):
    return [node["text"] for node in nodes] + [value for node in nodes for value in texts(node["children"])]

with tempfile.TemporaryDirectory(prefix="plico-vlist-") as temp:
    descriptor = Path(temp) / "active.json"
    first, second = Path(temp) / "first.so", Path(temp) / "second.so"
    shutil.copyfile(TARGET / "debug/libplico_gallery.so", first)
    shutil.copyfile(first, second)
    descriptor.write_text(json.dumps({"version": 2, "library": str(first)}))
    with open("/tmp/plico-vlist-reload.log", "w") as log:
        child = subprocess.Popen([str(TARGET / "debug/plico-backend"), "dev", "--module", str(descriptor), "--headless"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, text=True)
        messages = queue.Queue()
        def read():
            for line in child.stdout:
                messages.put(json.loads(line))
            messages.put(None)
        threading.Thread(target=read, daemon=True).start()
        try:
            assert messages.get(timeout=30)["event"] == "ready"
            def request(action="snapshot", **kwargs):
                child.stdin.write(json.dumps({"action": action, **kwargs}) + "\n")
                child.stdin.flush()
                reply = messages.get(timeout=30)
                assert reply is not None, "Host exited; see /tmp/plico-vlist-reload.log"
                assert not reply.get("error"), reply.get("error")
                return reply
            request("click", role="button", label="Lists")
            before = request()
            assert "Record 00001" in texts(before["nodes"])
            request("scroll", x=250, y=250, delta=3600)
            # This page opts into a host-clock-driven 160ms scroll transition.
            time.sleep(0.2)
            scrolled = request()
            assert "Record 00101" in texts(scrolled["nodes"])
            assert "Record 00001" not in texts(scrolled["nodes"])
            descriptor.write_text(json.dumps({"version": 2, "library": str(second)}))
            reloaded = request()
            assert reloaded["reloads"] == 1
            assert "Record 00101" in texts(reloaded["nodes"])
            assert "Record 00001" not in texts(reloaded["nodes"])
            # Activate another image while the host is still animating.
            request("scroll", x=250, y=250, delta=3600)
            third = Path(temp) / "third.so"
            shutil.copyfile(first, third)
            descriptor.write_text(json.dumps({"version": 2, "library": str(third)}))
            assert request()["reloads"] == 2
            time.sleep(0.2)
            continued = request()
            assert "Record 00201" in texts(continued["nodes"])
            out = ROOT / "reports/virtual-list/reload.json"
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(json.dumps({"scroll": True, "activation_preserves_window": True, "activation_preserves_scroll_motion": True}, indent=2) + "\n")
            print("Virtual list cdylib scrolling and activation passed")
        finally:
            child.stdin.close()
            child.wait(timeout=10)
